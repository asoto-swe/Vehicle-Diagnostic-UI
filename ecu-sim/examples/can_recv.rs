use socketcan::{CanSocket, EmbeddedFrame, Socket};

fn main() -> std::io::Result<()> {
    let sock = CanSocket::open("vcan0")?;
    println!("listening on vcan0...");

    loop {
        let frame = sock.read_frame()?;
        println!(
            "recv: id={:?} dlc={} data={:02x?}",
            frame.id(),
            frame.dlc(),
            frame.data()
        );
    }
}
