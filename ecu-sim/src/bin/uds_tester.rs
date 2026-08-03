use ecu_sim::isotp::IsoTpSocket;
use std::env;

struct EcuTarget {
    tx: u16,
    rx: u16,
}

fn resolve_ecu(name: &str) -> Option<EcuTarget> {
    match name {
        "bms" => Some(EcuTarget { tx: 0x7E0, rx: 0x7E8 }),
        "motor" => Some(EcuTarget { tx: 0x7E1, rx: 0x7E9 }),
        "thermal" => Some(EcuTarget { tx: 0x7E2, rx: 0x7EA }),
        _ => None,
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: uds_tester <bms|motor|thermal> <session [default|extended]|scan|clear|read <did-hex>|unlock|routine <start|stop|results>>"
    );
    std::process::exit(1);
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let Some(target) = args.get(1).and_then(|n| resolve_ecu(n)) else { usage() };
    let Some(cmd) = args.get(2).map(String::as_str) else { usage() };

    let sock = IsoTpSocket::open("vcan0", target.tx, target.rx)?;

    match cmd {
        "unlock" => return unlock(&sock),
        "routine" => {
            let sub = args.get(3).map(String::as_str).unwrap_or("start");
            return routine(&sock, sub);
        }
        _ => {}
    }

    let request: Vec<u8> = match cmd {
        "session" => {
            let extended = args.get(3).map(String::as_str) != Some("default");
            vec![0x10, if extended { 0x03 } else { 0x01 }]
        }
        "scan" => vec![0x19, 0x02],
        "clear" => vec![0x14, 0xFF, 0xFF, 0xFF],
        "read" => {
            let did = args
                .get(3)
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0xF190);
            vec![0x22, (did >> 8) as u8, (did & 0xFF) as u8]
        }
        _ => usage(),
    };

    println!("-> request:  {:02x?}", request);
    sock.send(&request)?;
    let resp = sock.receive()?;
    println!("<- response: {:02x?}", resp);
    print_decoded(&resp);
    Ok(())
}

fn unlock(sock: &IsoTpSocket) -> std::io::Result<()> {
    let seed_req = vec![0x27, 0x01];
    println!("-> request:  {:02x?}", seed_req);
    sock.send(&seed_req)?;
    let seed_resp = sock.receive()?;
    println!("<- response: {:02x?}", seed_resp);
    if seed_resp.first() != Some(&0x67) || seed_resp.len() < 6 {
        print_decoded(&seed_resp);
        return Ok(());
    }

    let seed = u32::from_be_bytes([seed_resp[2], seed_resp[3], seed_resp[4], seed_resp[5]]);
    let key = seed ^ 0xA5A5_A5A5;
    let mut key_req = vec![0x27, 0x02];
    key_req.extend_from_slice(&key.to_be_bytes());
    println!("-> request:  {:02x?}", key_req);
    sock.send(&key_req)?;
    let key_resp = sock.receive()?;
    println!("<- response: {:02x?}", key_resp);
    if key_resp.first() == Some(&0x67) {
        println!("security access granted");
    } else {
        print_decoded(&key_resp);
    }
    Ok(())
}

fn routine(sock: &IsoTpSocket, sub_name: &str) -> std::io::Result<()> {
    let sub = match sub_name {
        "start" => 0x01,
        "stop" => 0x02,
        "results" => 0x03,
        _ => usage(),
    };
    let req = vec![0x31, sub, 0xFF, 0x00]; // cooling pump test
    println!("-> request:  {:02x?}", req);
    sock.send(&req)?;
    let resp = sock.receive()?;
    println!("<- response: {:02x?}", resp);
    if resp.first() == Some(&0x71) {
        match resp.get(4) {
            Some(&result) => println!("routine result: {}", if result == 0 { "passed" } else { "failed" }),
            None => println!("routine acknowledged"),
        }
    } else {
        print_decoded(&resp);
    }
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
            let dtcs = resp.get(3..).unwrap_or(&[]);
            if dtcs.is_empty() {
                println!("  (no DTCs)");
            }
            for dtc in dtcs.chunks_exact(4) {
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
