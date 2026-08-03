use std::collections::HashMap;
use std::io;
use std::sync::mpsc as std_mpsc;
use std::thread;
use tokio::sync::oneshot;
use uds_transport::isotp::IsoTpSocket;

type PendingReply = oneshot::Sender<io::Result<Vec<u8>>>;

/// A handle to a background thread that owns one blocking IsoTpSocket for
/// one ECU. Axum handlers are async; ISO-TP/CAN I/O is blocking, so each
/// ECU gets its own OS thread and requests are funneled through a channel
/// rather than calling into the socket directly from async code.
#[derive(Clone)]
pub struct EcuHandle {
    tx: std_mpsc::Sender<(Vec<u8>, PendingReply)>,
}

impl EcuHandle {
    pub async fn request(&self, req: Vec<u8>) -> io::Result<Vec<u8>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send((req, reply_tx))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "ECU client thread is gone"))?;
        reply_rx
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "ECU client thread dropped the reply"))?
    }
}

fn spawn_ecu_client(iface: &str, tx_id: u16, rx_id: u16) -> EcuHandle {
    let (tx, rx) = std_mpsc::channel::<(Vec<u8>, PendingReply)>();
    let iface = iface.to_string();
    thread::spawn(move || {
        let sock = match IsoTpSocket::open(&iface, tx_id, rx_id) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to open isotp socket tx=0x{tx_id:03x} rx=0x{rx_id:03x}: {e}");
                return;
            }
        };
        while let Ok((req, reply)) = rx.recv() {
            let result = sock.send(&req).and_then(|_| sock.receive());
            let _ = reply.send(result);
        }
    });
    EcuHandle { tx }
}

pub type EcuRegistry = HashMap<&'static str, EcuHandle>;

/// Client-side addressing must mirror ecu-sim's fault_engine::spawn_* (this
/// backend is the tester; tx here is the ECU's rx and vice versa).
pub fn spawn_all(iface: &str) -> EcuRegistry {
    let mut registry = EcuRegistry::new();
    registry.insert("bms", spawn_ecu_client(iface, 0x7E0, 0x7E8));
    registry.insert("motor", spawn_ecu_client(iface, 0x7E1, 0x7E9));
    registry.insert("thermal", spawn_ecu_client(iface, 0x7E2, 0x7EA));
    registry
}
