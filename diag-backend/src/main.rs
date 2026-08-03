use axum::{
    extract::{State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::interval};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Dtc {
    code: String,
    status: String,
    subsystem: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct VehicleState {
    battery_soc: f32,
    battery_temp_c: f32,
    coolant_temp_c: f32,
    motor_rpm: i32,
    coolant_pump_ok: bool,
    session: String,
    dtcs: Vec<Dtc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DiagnosticRequest {
    service_id: String,
    payload: Option<String>,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    ecu_url: String,
    state: Arc<Mutex<VehicleState>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = AppState {
        client: reqwest::Client::new(),
        ecu_url: "http://127.0.0.1:3031".into(),
        state: Arc::new(Mutex::new(VehicleState {
            battery_soc: 0.0,
            battery_temp_c: 0.0,
            coolant_temp_c: 0.0,
            motor_rpm: 0,
            coolant_pump_ok: true,
            session: "default".into(),
            dtcs: vec![],
        })),
    };

    tokio::spawn(stream_vehicle_state(state.clone()));

    let app = Router::new()
        .route("/health", get(health))
        .route("/diagnostics", get(get_diagnostics))
        .route("/diagnostics/scan", post(scan_faults))
        .route("/diagnostics/clear", post(clear_faults))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind backend port");

    println!("Backend listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.expect("server failure");
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn get_diagnostics(State(state): State<AppState>) -> Json<VehicleState> {
    let vehicle = state.state.lock().await;
    Json(vehicle.clone())
}

async fn scan_faults(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let request = DiagnosticRequest {
        service_id: "0x19".into(),
        payload: None,
    };
    let response = state.client.post(format!("{}/diagnostic", state.ecu_url)).json(&request).send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let body: serde_json::Value = response.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(body))
}

async fn clear_faults(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let response = state.client.post(format!("{}/clear", state.ecu_url)).send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let body: serde_json::Value = response.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(body))
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| async move {
        let mut interval = interval(Duration::from_secs(1));
        let mut stream = socket;
        loop {
            interval.tick().await;
            let vehicle = state.state.lock().await.clone();
            let payload = serde_json::to_string(&vehicle).unwrap();
            if stream.send(axum::extract::ws::Message::Text(payload.into())).await.is_err() {
                break;
            }
        }
    })
}

async fn stream_vehicle_state(state: AppState) {
    let mut interval = interval(Duration::from_millis(800));
    loop {
        interval.tick().await;
        let response = state.client.get(format!("{}/status", state.ecu_url)).send().await;
        if let Ok(response) = response {
            if let Ok(vehicle) = response.json::<VehicleState>().await {
                let mut shared = state.state.lock().await;
                *shared = vehicle;
            }
        }
    }
}

use axum::http::StatusCode;
