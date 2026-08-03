use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::{sync::Mutex, time::{sleep, Duration}};

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

#[derive(Clone, Debug, Deserialize)]
struct DiagnosticRequest {
    service_id: String,
    payload: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct DiagnosticResponse {
    service_id: String,
    status: String,
    payload: serde_json::Value,
}

#[derive(Clone)]
struct AppState {
    vehicle: Arc<Mutex<VehicleState>>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        vehicle: Arc::new(Mutex::new(VehicleState {
            battery_soc: 88.0,
            battery_temp_c: 38.0,
            coolant_temp_c: 55.0,
            motor_rpm: 3200,
            coolant_pump_ok: true,
            session: "default".into(),
            dtcs: vec![
                Dtc {
                    code: "P0A80".into(),
                    status: "active".into(),
                    subsystem: "battery".into(),
                },
                Dtc {
                    code: "C1234".into(),
                    status: "pending".into(),
                    subsystem: "powertrain".into(),
                },
            ],
        })),
    };

    tokio::spawn(run_fault_engine(state.clone()));

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/diagnostic", post(handle_diagnostic))
        .route("/clear", post(clear_diagnostics))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3031")
        .await
        .expect("failed to bind to simulator port");

    println!("ECU simulator listening on http://127.0.0.1:3031");
    axum::serve(listener, app).await.expect("server failure");
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn status(State(state): State<AppState>) -> Json<VehicleState> {
    let vehicle = state.vehicle.lock().await;
    Json(vehicle.clone())
}

async fn clear_diagnostics(State(state): State<AppState>) -> Json<DiagnosticResponse> {
    let mut vehicle = state.vehicle.lock().await;
    vehicle.dtc_clear();
    Json(DiagnosticResponse {
        service_id: "0x14".into(),
        status: "cleared".into(),
        payload: serde_json::json!({"cleared": true}),
    })
}

async fn handle_diagnostic(
    State(state): State<AppState>,
    Json(request): Json<DiagnosticRequest>,
) -> Result<Json<DiagnosticResponse>, StatusCode> {
    let mut vehicle = state.vehicle.lock().await;
    match request.service_id.as_str() {
        "0x10" => {
            vehicle.session = "extended".into();
            Ok(Json(DiagnosticResponse {
                service_id: request.service_id,
                status: "ok".into(),
                payload: serde_json::json!({"session": vehicle.session}),
            }))
        }
        "0x22" => {
            let data = match request.payload.as_deref() {
                Some("battery") => serde_json::json!({"battery_soc": vehicle.battery_soc, "battery_temp_c": vehicle.battery_temp_c}),
                Some("motor") => serde_json::json!({"motor_rpm": vehicle.motor_rpm}),
                Some("coolant") => serde_json::json!({"coolant_temp_c": vehicle.coolant_temp_c, "pump_ok": vehicle.coolant_pump_ok}),
                _ => serde_json::json!({"message": "unknown data id"}),
            };
            Ok(Json(DiagnosticResponse {
                service_id: request.service_id,
                status: "ok".into(),
                payload: data,
            }))
        }
        "0x19" => Ok(Json(DiagnosticResponse {
            service_id: request.service_id,
            status: "ok".into(),
            payload: serde_json::json!({"dtcs": vehicle.dtcs}),
        })),
        "0x14" => {
            vehicle.dtc_clear();
            Ok(Json(DiagnosticResponse {
                service_id: request.service_id,
                status: "ok".into(),
                payload: serde_json::json!({"cleared": true}),
            }))
        }
        "0x27" => Ok(Json(DiagnosticResponse {
            service_id: request.service_id,
            status: "ok".into(),
            payload: serde_json::json!({"level": "engineering"}),
        })),
        "0x31" => Ok(Json(DiagnosticResponse {
            service_id: request.service_id,
            status: "ok".into(),
            payload: serde_json::json!({"routine": "cooling_pump_test", "result": "passed"}),
        })),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn run_fault_engine(state: AppState) {
    let mut tick = 0u32;
    loop {
        sleep(Duration::from_secs(2)).await;
        let mut vehicle = state.vehicle.lock().await;
        tick += 1;
        vehicle.motor_rpm = (3000 + (tick as i32 % 7) * 250).clamp(3000, 5000);
        if !vehicle.coolant_pump_ok {
            vehicle.coolant_temp_c += 1.2;
            vehicle.battery_temp_c += 0.6;
        } else if tick % 3 == 0 {
            vehicle.coolant_temp_c += 0.4;
            vehicle.battery_temp_c += 0.2;
        }

        if vehicle.coolant_temp_c > 70.0 && !vehicle.dtcs.iter().any(|dtc| dtc.code == "P0218") {
            vehicle.dtcs.push(Dtc {
                code: "P0218".into(),
                status: "active".into(),
                subsystem: "thermal".into(),
            });
        }

        if vehicle.battery_temp_c > 60.0 && !vehicle.dtcs.iter().any(|dtc| dtc.code == "P0A80") {
            vehicle.dtcs.push(Dtc {
                code: "P0A80".into(),
                status: "active".into(),
                subsystem: "battery".into(),
            });
        }

        if vehicle.motor_rpm > 4200 && !vehicle.dtcs.iter().any(|dtc| dtc.code == "C1234") {
            vehicle.dtcs.push(Dtc {
                code: "C1234".into(),
                status: "pending".into(),
                subsystem: "powertrain".into(),
            });
        }

        if vehicle.coolant_temp_c > 75.0 && vehicle.coolant_pump_ok {
            vehicle.coolant_pump_ok = false;
            vehicle.dtcs.push(Dtc {
                code: "U0155".into(),
                status: "active".into(),
                subsystem: "sensor".into(),
            });
        }
        if vehicle.coolant_temp_c < 40.0 {
            vehicle.coolant_pump_ok = true;
        }
    }
}

impl VehicleState {
    fn dtc_clear(&mut self) {
        self.dtcs.clear();
    }
}
