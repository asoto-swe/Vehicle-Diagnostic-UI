use socketcan::{CanFrame, CanSocket, EmbeddedFrame, Socket, StandardId};
use std::thread::sleep;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let sock = CanSocket::open("vcan0")?;
    let id = StandardId::new(0x123).expect("0x123 is a valid 11-bit CAN ID");

    let mut counter: u8 = 0;
    loop {
        let payload = [0xDE, 0xAD, 0xBE, 0xEF, counter, 0, 0, 0];
        let frame = CanFrame::new(id, &payload).expect("payload fits in a classic CAN frame");
        sock.write_frame(&frame)?;
        println!("sent: id={:?} data={:02x?}", frame.id(), frame.data());
        counter = counter.wrapping_add(1);
        sleep(Duration::from_secs(1));
    }
}
