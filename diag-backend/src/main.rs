mod uds_client;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tower_http::cors::CorsLayer;
use uds_client::EcuRegistry;
use uds_transport::uds;

#[derive(Clone)]
struct AppState {
    ecus: Arc<EcuRegistry>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = AppState { ecus: Arc::new(uds_client::spawn_all("vcan0")) };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ecus", get(list_ecus))
        .route("/ecus/{name}/session", post(session))
        .route("/ecus/{name}/dtcs", get(get_dtcs))
        .route("/ecus/{name}/dtcs/clear", post(clear_dtcs))
        .route("/ecus/{name}/data/{did}", get(read_data))
        .route("/ecus/{name}/security/unlock", post(unlock))
        .route("/ecus/{name}/routine", post(routine))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind backend port");
    println!("diag-backend listening on http://127.0.0.1:3000 (UDS over vcan0)");
    axum::serve(listener, app).await.expect("server failure");
}

async fn health() -> &'static str {
    "ok"
}

async fn list_ecus() -> Json<Vec<&'static str>> {
    Json(vec!["bms", "motor", "thermal"])
}

async fn ecu_request_raw(state: &AppState, name: &str, req: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let Some(handle) = state.ecus.get(name) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("unknown ecu '{name}'")));
    };
    handle.request(req).await
}

async fn ecu_request(state: &AppState, name: &str, req: Vec<u8>) -> Result<Vec<u8>, Response> {
    ecu_request_raw(state, name, req).await.map_err(|e| {
        let status = if e.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_GATEWAY
        };
        (status, Json(json!({ "error": e.to_string() }))).into_response()
    })
}

fn is_negative(resp: &[u8]) -> bool {
    resp.first() == Some(&uds::NEGATIVE_RESPONSE_SID)
}

fn negative_response_json(resp: &[u8]) -> Response {
    let sid = resp.get(1).copied().unwrap_or(0);
    let nrc = resp.get(2).copied().unwrap_or(0);
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": "negative response", "service": format!("0x{sid:02x}"), "nrc": format!("0x{nrc:02x}") })),
    )
        .into_response()
}

#[derive(Serialize)]
struct DtcOut {
    code: String,
    status: u8,
    test_failed: bool,
    pending: bool,
    confirmed: bool,
}

fn parse_dtcs(resp: &[u8]) -> Vec<DtcOut> {
    resp.get(3..)
        .unwrap_or(&[])
        .chunks_exact(4)
        .map(|c| {
            let code = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
            DtcOut {
                code: format!("{code:06X}"),
                status: c[3],
                test_failed: c[3] & uds::DTC_STATUS_TEST_FAILED != 0,
                pending: c[3] & uds::DTC_STATUS_PENDING != 0,
                confirmed: c[3] & uds::DTC_STATUS_CONFIRMED != 0,
            }
        })
        .collect()
}

#[derive(Deserialize)]
struct SessionBody {
    extended: bool,
}

async fn session(State(state): State<AppState>, Path(name): Path<String>, Json(body): Json<SessionBody>) -> Response {
    let sub = if body.extended { uds::SESSION_EXTENDED } else { uds::SESSION_DEFAULT };
    let resp = match ecu_request(&state, &name, vec![uds::SID_DIAGNOSTIC_SESSION_CONTROL, sub]).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&resp) {
        return negative_response_json(&resp);
    }
    Json(json!({ "session": resp.get(1).copied().unwrap_or(0) })).into_response()
}

async fn get_dtcs(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let req = vec![uds::SID_READ_DTC_INFORMATION, uds::REPORT_DTC_BY_STATUS_MASK];
    let resp = match ecu_request(&state, &name, req).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&resp) {
        return negative_response_json(&resp);
    }
    Json(parse_dtcs(&resp)).into_response()
}

async fn clear_dtcs(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let req = vec![uds::SID_CLEAR_DIAGNOSTIC_INFORMATION, 0xFF, 0xFF, 0xFF];
    let resp = match ecu_request(&state, &name, req).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&resp) {
        return negative_response_json(&resp);
    }
    Json(json!({ "cleared": true })).into_response()
}

async fn read_data(State(state): State<AppState>, Path((name, did)): Path<(String, String)>) -> Response {
    let Ok(did_val) = u16::from_str_radix(did.trim_start_matches("0x"), 16) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "did must be hex, e.g. 1000 or 0xF190" }))).into_response();
    };
    let req = vec![uds::SID_READ_DATA_BY_IDENTIFIER, (did_val >> 8) as u8, (did_val & 0xFF) as u8];
    let resp = match ecu_request(&state, &name, req).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&resp) {
        return negative_response_json(&resp);
    }
    let data = resp.get(3..).unwrap_or(&[]);
    Json(json!({
        "did": format!("{did_val:04X}"),
        "hex": data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(""),
        "bytes": data,
    }))
    .into_response()
}

async fn unlock(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let seed_resp = match ecu_request(&state, &name, vec![uds::SID_SECURITY_ACCESS, uds::SECURITY_REQUEST_SEED]).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&seed_resp) {
        return negative_response_json(&seed_resp);
    }
    if seed_resp.len() < 6 {
        return (StatusCode::BAD_GATEWAY, Json(json!({ "error": "malformed seed response" }))).into_response();
    }

    // Deliberately trivial "algorithm" matching ecus::thermal — the only
    // ECU in this demo that implements Security Access.
    let seed = u32::from_be_bytes([seed_resp[2], seed_resp[3], seed_resp[4], seed_resp[5]]);
    let key = seed ^ 0xA5A5_A5A5;
    let mut key_req = vec![uds::SID_SECURITY_ACCESS, uds::SECURITY_SEND_KEY];
    key_req.extend_from_slice(&key.to_be_bytes());

    let key_resp = match ecu_request(&state, &name, key_req).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&key_resp) {
        return negative_response_json(&key_resp);
    }
    Json(json!({ "unlocked": true })).into_response()
}

#[derive(Deserialize)]
struct RoutineBody {
    action: String,
}

async fn routine(State(state): State<AppState>, Path(name): Path<String>, Json(body): Json<RoutineBody>) -> Response {
    let sub = match body.action.as_str() {
        "start" => uds::ROUTINE_START,
        "stop" => uds::ROUTINE_STOP,
        "results" => uds::ROUTINE_REQUEST_RESULTS,
        _ => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "action must be start|stop|results" }))).into_response(),
    };
    let req = vec![uds::SID_ROUTINE_CONTROL, sub, 0xFF, 0x00]; // cooling pump test
    let resp = match ecu_request(&state, &name, req).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if is_negative(&resp) {
        return negative_response_json(&resp);
    }
    let result = resp.get(4).map(|&r| if r == 0 { "passed" } else { "failed" });
    Json(json!({ "routine": "cooling_pump_test", "result": result })).into_response()
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| stream_telemetry(socket, state))
}

#[derive(Serialize)]
struct VehicleSnapshot {
    bms: Option<EcuSnapshot>,
    motor: Option<EcuSnapshot>,
    thermal: Option<EcuSnapshot>,
}

#[derive(Serialize)]
struct EcuSnapshot {
    dtcs: Vec<DtcOut>,
    data: serde_json::Value,
}

#[derive(Clone, Copy)]
enum DidWidth {
    U8,
    U16,
}

async fn stream_telemetry(mut socket: WebSocket, state: AppState) {
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        let snapshot = VehicleSnapshot {
            bms: snapshot_ecu(&state, "bms", &[("battery_temp_c", 0x1000, DidWidth::U8), ("battery_soc", 0x1001, DidWidth::U8)])
                .await,
            motor: snapshot_ecu(&state, "motor", &[("rpm", 0x1010, DidWidth::U16)]).await,
            thermal: snapshot_ecu(
                &state,
                "thermal",
                &[("coolant_temp_c", 0x1002, DidWidth::U8), ("pump_ok", 0x1003, DidWidth::U8)],
            )
            .await,
        };
        let Ok(payload) = serde_json::to_string(&snapshot) else { continue };
        if socket.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

async fn snapshot_ecu(state: &AppState, name: &str, dids: &[(&str, u16, DidWidth)]) -> Option<EcuSnapshot> {
    let dtc_resp = ecu_request_raw(state, name, vec![uds::SID_READ_DTC_INFORMATION, uds::REPORT_DTC_BY_STATUS_MASK])
        .await
        .ok()?;
    let dtcs = if is_negative(&dtc_resp) { vec![] } else { parse_dtcs(&dtc_resp) };

    let mut data = serde_json::Map::new();
    for (key, did, width) in dids {
        let req = vec![uds::SID_READ_DATA_BY_IDENTIFIER, (*did >> 8) as u8, (*did & 0xFF) as u8];
        let Ok(resp) = ecu_request_raw(state, name, req).await else { continue };
        if is_negative(&resp) {
            continue;
        }
        let payload = resp.get(3..).unwrap_or(&[]);
        let value = match width {
            DidWidth::U8 => payload.first().map(|&b| json!(b)),
            DidWidth::U16 => (payload.len() >= 2).then(|| json!(((payload[0] as u16) << 8) | payload[1] as u16)),
        };
        if let Some(v) = value {
            data.insert((*key).to_string(), v);
        }
    }

    Some(EcuSnapshot { dtcs, data: serde_json::Value::Object(data) })
}
