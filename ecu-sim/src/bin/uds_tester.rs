use ecu_sim::isotp::IsoTpSocket;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");

    let request: Vec<u8> = match cmd {
        "session" => {
            let extended = args.get(2).map(String::as_str) != Some("default");
            vec![0x10, if extended { 0x03 } else { 0x01 }]
        }
        "scan" => vec![0x19, 0x02],
        "clear" => vec![0x14, 0xFF, 0xFF, 0xFF],
        "read" => {
            let did = args
                .get(2)
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0xF190);
            vec![0x22, (did >> 8) as u8, (did & 0xFF) as u8]
        }
        _ => {
            eprintln!("usage: uds_tester <session [default|extended]|scan|clear|read [did-hex]>");
            std::process::exit(1);
        }
    };

    let sock = IsoTpSocket::open("vcan0", 0x7E0, 0x7E8)?;
    println!("-> request:  {:02x?}", request);
    sock.send(&request)?;
    let resp = sock.receive()?;
    println!("<- response: {:02x?}", resp);
    print_decoded(&resp);
    Ok(())
}

fn print_decoded(resp: &[u8]) {
    match resp.first() {
        Some(0x7F) => println!(
            "NEGATIVE RESPONSE: service=0x{:02x} nrc=0x{:02x}",
            resp.get(1).unwrap_or(&0),
            resp.get(2).unwrap_or(&0)
        ),
        Some(0x50) => println!("session control OK, session=0x{:02x}", resp.get(1).unwrap_or(&0)),
        Some(0x54) => println!("DTCs cleared"),
        Some(0x59) => {
            for dtc in resp.get(3..).unwrap_or(&[]).chunks_exact(4) {
                println!("  DTC {:02x}{:02x}{:02x} status=0x{:02x}", dtc[0], dtc[1], dtc[2], dtc[3]);
            }
        }
        Some(0x62) => println!(
            "DID 0x{:02x}{:02x} = {:02x?}",
            resp.get(1).unwrap_or(&0),
            resp.get(2).unwrap_or(&0),
            resp.get(3..).unwrap_or(&[])
        ),
        _ => println!("unrecognized response"),
    }
}
