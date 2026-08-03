use ecu_sim::isotp::IsoTpSocket;

// ECU side: listens on 0x7E0 (tester requests), replies/flow-controls on 0x7E8.
fn main() -> std::io::Result<()> {
    let sock = IsoTpSocket::open("vcan0", 0x7E8, 0x7E0)?;
    println!("listening on vcan0 (rx=0x7E0)...");

    loop {
        let payload = sock.receive()?;
        println!("received {} bytes: {:02x?}", payload.len(), payload);
    }
}
