use ecu_sim::isotp::IsoTpSocket;

// Tester -> ECU on 0x7E0, ECU -> tester on 0x7E8 (standard UDS physical addressing).
fn main() -> std::io::Result<()> {
    let sock = IsoTpSocket::open("vcan0", 0x7E0, 0x7E8)?;

    let payload: Vec<u8> = (0..20).collect();
    println!("sending {} bytes: {:02x?}", payload.len(), payload);
    sock.send(&payload)?;
    println!("done");
    Ok(())
}
