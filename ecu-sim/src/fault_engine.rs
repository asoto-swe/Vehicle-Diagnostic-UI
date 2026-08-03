use crate::ecus::bms::BmsEcu;
use crate::ecus::motor::MotorEcu;
use crate::ecus::thermal::ThermalEcu;
use crate::isotp::IsoTpSocket;
use socketcan::{CanFrame, CanSocket, EmbeddedFrame, Id, Socket, StandardId};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Not a UDS message: a plain periodic status broadcast, the way real ECUs
/// share state with each other on the bus (not just respond to a tester).
pub const THERMAL_BROADCAST_ID: u16 = 0x300;

const TICK_INTERVAL: Duration = Duration::from_millis(500);

pub fn spawn_bms(iface: &str) -> Arc<Mutex<BmsEcu>> {
    let ecu = Arc::new(Mutex::new(BmsEcu::new()));

    spawn_request_loop(iface, 0x7E8, 0x7E0, ecu.clone(), BmsEcu::handle_request);

    {
        let ecu = ecu.clone();
        thread::spawn(move || loop {
            thread::sleep(TICK_INTERVAL);
            ecu.lock().unwrap().tick();
        });
    }

    {
        let ecu = ecu.clone();
        let iface = iface.to_string();
        thread::spawn(move || {
            let sock = CanSocket::open(&iface).expect("open BMS broadcast listener");
            loop {
                let Ok(frame) = sock.read_frame() else { continue };
                if raw_id(frame.id()) != Some(THERMAL_BROADCAST_ID) {
                    continue;
                }
                let data = frame.data();
                if data.len() < 2 {
                    continue;
                }
                let pump_ok = data[0] != 0;
                let coolant_temp_c = data[1] as f32;
                ecu.lock().unwrap().apply_coolant_influence(coolant_temp_c, pump_ok);
            }
        });
    }

    ecu
}

pub fn spawn_motor(iface: &str) -> Arc<Mutex<MotorEcu>> {
    let ecu = Arc::new(Mutex::new(MotorEcu::new()));

    spawn_request_loop(iface, 0x7E9, 0x7E1, ecu.clone(), MotorEcu::handle_request);

    {
        let ecu = ecu.clone();
        thread::spawn(move || loop {
            thread::sleep(TICK_INTERVAL);
            ecu.lock().unwrap().tick();
        });
    }

    ecu
}

pub fn spawn_thermal(iface: &str) -> Arc<Mutex<ThermalEcu>> {
    let ecu = Arc::new(Mutex::new(ThermalEcu::new()));

    spawn_request_loop(iface, 0x7EA, 0x7E2, ecu.clone(), ThermalEcu::handle_request);

    {
        let ecu = ecu.clone();
        let iface = iface.to_string();
        thread::spawn(move || {
            let sock = CanSocket::open(&iface).expect("open thermal broadcast socket");
            let id = StandardId::new(THERMAL_BROADCAST_ID).expect("valid broadcast id");
            loop {
                thread::sleep(TICK_INTERVAL);
                let payload = {
                    let mut guard = ecu.lock().unwrap();
                    guard.tick();
                    guard.status_broadcast_payload()
                };
                if let Some(frame) = CanFrame::new(id, &payload) {
                    let _ = sock.write_frame(&frame);
                }
            }
        });
    }

    ecu
}

fn spawn_request_loop<E, F>(iface: &str, tx_id: u16, rx_id: u16, ecu: Arc<Mutex<E>>, handle: F)
where
    E: Send + 'static,
    F: Fn(&mut E, &[u8]) -> Vec<u8> + Send + 'static,
{
    let iface = iface.to_string();
    thread::spawn(move || {
        let sock = IsoTpSocket::open(&iface, tx_id, rx_id).expect("open ECU isotp socket");
        loop {
            let Ok(req) = sock.receive() else { continue };
            let resp = handle(&mut ecu.lock().unwrap(), &req);
            let _ = sock.send(&resp);
        }
    });
}

fn raw_id(id: Id) -> Option<u16> {
    match id {
        Id::Standard(s) => Some(s.as_raw()),
        Id::Extended(_) => None,
    }
}
