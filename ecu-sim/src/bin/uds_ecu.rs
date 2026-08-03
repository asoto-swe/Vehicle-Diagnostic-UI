use ecu_sim::isotp::IsoTpSocket;
use ecu_sim::uds::SimulatedEcu;

fn main() -> std::io::Result<()> {
    let sock = IsoTpSocket::open("vcan0", 0x7E8, 0x7E0)?;
    let mut ecu = SimulatedEcu::new();
    println!("UDS ECU simulator listening on vcan0 (rx=0x7E0, tx=0x7E8)");

    loop {
        let req = sock.receive()?;
        println!("<- request:  {:02x?}", req);
        let resp = ecu.handle_request(&req);
        println!("-> response: {:02x?}", resp);
        sock.send(&resp)?;
    }
}
