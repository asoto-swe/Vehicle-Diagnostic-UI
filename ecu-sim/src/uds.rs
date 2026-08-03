pub const SID_DIAGNOSTIC_SESSION_CONTROL: u8 = 0x10;
pub const SID_CLEAR_DIAGNOSTIC_INFORMATION: u8 = 0x14;
pub const SID_READ_DTC_INFORMATION: u8 = 0x19;
pub const SID_READ_DATA_BY_IDENTIFIER: u8 = 0x22;
pub const NEGATIVE_RESPONSE_SID: u8 = 0x7F;

pub const SESSION_DEFAULT: u8 = 0x01;
pub const SESSION_EXTENDED: u8 = 0x03;

pub const REPORT_DTC_BY_STATUS_MASK: u8 = 0x02;

pub const NRC_SERVICE_NOT_SUPPORTED: u8 = 0x11;
pub const NRC_SUBFUNCTION_NOT_SUPPORTED: u8 = 0x12;
pub const NRC_REQUEST_OUT_OF_RANGE: u8 = 0x31;

#[derive(Clone, Copy)]
pub struct Dtc {
    /// 3-byte DTC number as it appears on the wire, e.g. 0x0A8000.
    pub code: u32,
    pub status: u8,
}

pub struct SimulatedEcu {
    pub session: u8,
    pub dtcs: Vec<Dtc>,
}

impl SimulatedEcu {
    pub fn new() -> Self {
        Self {
            session: SESSION_DEFAULT,
            dtcs: vec![
                Dtc { code: 0x0A_8000, status: 0x08 }, // ~P0A80 battery over-temp, confirmed
                Dtc { code: 0x02_1800, status: 0x08 }, // ~P0218 coolant over-temp, confirmed
            ],
        }
    }

    pub fn handle_request(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sid) = req.first() else {
            return negative_response(0x00, NRC_SERVICE_NOT_SUPPORTED);
        };
        match sid {
            SID_DIAGNOSTIC_SESSION_CONTROL => self.session_control(req),
            SID_CLEAR_DIAGNOSTIC_INFORMATION => self.clear_dtcs(),
            SID_READ_DTC_INFORMATION => self.read_dtc_information(req),
            SID_READ_DATA_BY_IDENTIFIER => self.read_data_by_identifier(req),
            _ => negative_response(sid, NRC_SERVICE_NOT_SUPPORTED),
        }
    }

    fn session_control(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sub) = req.get(1) else {
            return negative_response(SID_DIAGNOSTIC_SESSION_CONTROL, NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        if sub != SESSION_DEFAULT && sub != SESSION_EXTENDED {
            return negative_response(SID_DIAGNOSTIC_SESSION_CONTROL, NRC_SUBFUNCTION_NOT_SUPPORTED);
        }
        self.session = sub;
        // positive response: SID+0x40, subfunction echo, P2 (ms, 2 bytes), P2* (x10ms, 2 bytes)
        vec![SID_DIAGNOSTIC_SESSION_CONTROL + 0x40, sub, 0x00, 0x32, 0x01, 0xF4]
    }

    fn clear_dtcs(&mut self) -> Vec<u8> {
        self.dtcs.clear();
        vec![SID_CLEAR_DIAGNOSTIC_INFORMATION + 0x40]
    }

    fn read_dtc_information(&self, req: &[u8]) -> Vec<u8> {
        let Some(&sub) = req.get(1) else {
            return negative_response(SID_READ_DTC_INFORMATION, NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        if sub != REPORT_DTC_BY_STATUS_MASK {
            return negative_response(SID_READ_DTC_INFORMATION, NRC_SUBFUNCTION_NOT_SUPPORTED);
        }
        let mut resp = vec![SID_READ_DTC_INFORMATION + 0x40, sub, 0xFF];
        for dtc in &self.dtcs {
            resp.push(((dtc.code >> 16) & 0xFF) as u8);
            resp.push(((dtc.code >> 8) & 0xFF) as u8);
            resp.push((dtc.code & 0xFF) as u8);
            resp.push(dtc.status);
        }
        resp
    }

    fn read_data_by_identifier(&self, req: &[u8]) -> Vec<u8> {
        let (Some(&hi), Some(&lo)) = (req.get(1), req.get(2)) else {
            return negative_response(SID_READ_DATA_BY_IDENTIFIER, NRC_REQUEST_OUT_OF_RANGE);
        };
        let did = ((hi as u16) << 8) | lo as u16;
        let data: &[u8] = match did {
            0xF190 => b"VDTSIM0001", // VIN-like identifier
            0x1001 => &[0x58],       // battery state of charge, percent
            _ => return negative_response(SID_READ_DATA_BY_IDENTIFIER, NRC_REQUEST_OUT_OF_RANGE),
        };
        let mut resp = vec![SID_READ_DATA_BY_IDENTIFIER + 0x40, hi, lo];
        resp.extend_from_slice(data);
        resp
    }
}

impl Default for SimulatedEcu {
    fn default() -> Self {
        Self::new()
    }
}

fn negative_response(sid: u8, nrc: u8) -> Vec<u8> {
    vec![NEGATIVE_RESPONSE_SID, sid, nrc]
}
