use socketcan::{CanFrame, CanSocket, EmbeddedFrame, Id, Socket, StandardId};
use std::io;
use std::thread::sleep;
use std::time::Duration;

const PCI_SINGLE_FRAME: u8 = 0x0;
const PCI_FIRST_FRAME: u8 = 0x1;
const PCI_CONSECUTIVE_FRAME: u8 = 0x2;
const PCI_FLOW_CONTROL: u8 = 0x3;

const FLOW_STATUS_CONTINUE: u8 = 0;
const FLOW_STATUS_OVERFLOW: u8 = 2;

/// ISO 15765-2 transport over a classic (non-FD) CAN interface, with 11-bit
/// standard addressing. Frames are always padded to 8 bytes, which is the
/// common (if not strictly mandatory) convention on real ECUs.
///
/// Simplifications versus the full spec: reads block with no timeout (a
/// non-responding peer hangs the caller), and WAIT flow-control frames are
/// not retried — only CONTINUE/OVERFLOW are handled.
pub struct IsoTpSocket {
    sock: CanSocket,
    tx_id: u16,
    rx_id: u16,
}

impl IsoTpSocket {
    pub fn open(iface: &str, tx_id: u16, rx_id: u16) -> io::Result<Self> {
        let sock = CanSocket::open(iface)?;
        Ok(Self { sock, tx_id, rx_id })
    }

    pub fn send(&self, data: &[u8]) -> io::Result<()> {
        if data.len() <= 7 {
            let mut payload = vec![(PCI_SINGLE_FRAME << 4) | data.len() as u8];
            payload.extend_from_slice(data);
            pad_to_8(&mut payload);
            return self.send_raw(&payload);
        }

        let total_len = data.len();
        if total_len > 0xFFF {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload too large for classic (non-FD) ISO-TP: max 4095 bytes",
            ));
        }

        let mut first_frame = vec![
            (PCI_FIRST_FRAME << 4) | ((total_len >> 8) as u8 & 0x0F),
            (total_len & 0xFF) as u8,
        ];
        first_frame.extend_from_slice(&data[0..6]);
        self.send_raw(&first_frame)?;

        let (mut block_size, mut st_min) = self.await_flow_control()?;
        let mut offset = 6;
        let mut seq: u8 = 1;
        let mut sent_in_block = 0u32;

        while offset < total_len {
            let end = (offset + 7).min(total_len);
            let mut cf = vec![(PCI_CONSECUTIVE_FRAME << 4) | (seq & 0x0F)];
            cf.extend_from_slice(&data[offset..end]);
            pad_to_8(&mut cf);
            self.send_raw(&cf)?;

            offset = end;
            seq = if seq == 15 { 0 } else { seq + 1 };
            sent_in_block += 1;

            if offset >= total_len {
                break;
            }

            if block_size != 0 && sent_in_block >= block_size as u32 {
                let (bs, st) = self.await_flow_control()?;
                block_size = bs;
                st_min = st;
                sent_in_block = 0;
            } else if !st_min.is_zero() {
                sleep(st_min);
            }
        }

        Ok(())
    }

    pub fn receive(&self) -> io::Result<Vec<u8>> {
        loop {
            let frame = self.read_from_rx()?;
            let data = frame.data();
            if data.is_empty() {
                continue;
            }
            match data[0] >> 4 {
                x if x == PCI_SINGLE_FRAME => {
                    let len = (data[0] & 0x0F) as usize;
                    return Ok(data[1..1 + len].to_vec());
                }
                x if x == PCI_FIRST_FRAME => {
                    return self.receive_multi_frame(data);
                }
                _ => continue,
            }
        }
    }

    fn receive_multi_frame(&self, first_frame_data: &[u8]) -> io::Result<Vec<u8>> {
        let total_len = (((first_frame_data[0] & 0x0F) as usize) << 8) | first_frame_data[1] as usize;
        let mut buf = Vec::with_capacity(total_len);
        buf.extend_from_slice(&first_frame_data[2..first_frame_data.len().min(8)]);

        let fc = vec![(PCI_FLOW_CONTROL << 4) | FLOW_STATUS_CONTINUE, 0, 0, 0, 0, 0, 0, 0];
        self.send_raw(&fc)?;

        let mut expected_seq: u8 = 1;
        while buf.len() < total_len {
            let frame = self.read_from_rx()?;
            let cf = frame.data();
            if cf.is_empty() || (cf[0] >> 4) != PCI_CONSECUTIVE_FRAME {
                continue;
            }
            let seq = cf[0] & 0x0F;
            if seq != expected_seq {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("out-of-order consecutive frame: expected {expected_seq}, got {seq}"),
                ));
            }
            let remaining = total_len - buf.len();
            let take = remaining.min(cf.len() - 1);
            buf.extend_from_slice(&cf[1..1 + take]);
            expected_seq = if expected_seq == 15 { 0 } else { expected_seq + 1 };
        }

        Ok(buf)
    }

    fn await_flow_control(&self) -> io::Result<(u8, Duration)> {
        let frame = self.read_from_rx()?;
        let fc = frame.data();
        if fc.is_empty() || (fc[0] >> 4) != PCI_FLOW_CONTROL {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "expected flow control frame"));
        }
        if (fc[0] & 0x0F) == FLOW_STATUS_OVERFLOW {
            return Err(io::Error::new(io::ErrorKind::Other, "receiver reported buffer overflow"));
        }
        let block_size = fc.get(1).copied().unwrap_or(0);
        let st_min = decode_st_min(fc.get(2).copied().unwrap_or(0));
        Ok((block_size, st_min))
    }

    fn send_raw(&self, payload: &[u8]) -> io::Result<()> {
        let id = StandardId::new(self.tx_id).expect("tx_id must be a valid 11-bit CAN id");
        let frame = CanFrame::new(id, payload).expect("payload must fit in one CAN frame");
        self.sock.write_frame(&frame)
    }

    fn read_from_rx(&self) -> io::Result<CanFrame> {
        loop {
            let frame = self.sock.read_frame()?;
            if frame_id_raw(frame.id()) == Some(self.rx_id) {
                return Ok(frame);
            }
        }
    }
}

fn frame_id_raw(id: Id) -> Option<u16> {
    match id {
        Id::Standard(s) => Some(s.as_raw()),
        Id::Extended(_) => None,
    }
}

fn pad_to_8(payload: &mut Vec<u8>) {
    while payload.len() < 8 {
        payload.push(0x00);
    }
}

fn decode_st_min(raw: u8) -> Duration {
    match raw {
        0x00..=0x7F => Duration::from_millis(raw as u64),
        0xF1..=0xF9 => Duration::from_micros((raw as u64 - 0xF0) * 100),
        _ => Duration::from_millis(0),
    }
}
