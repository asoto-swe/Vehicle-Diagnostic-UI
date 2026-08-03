use crate::uds::{self, Dtc};

const DTC_OVER_REV: u32 = 0x08_1234; // illustrative, not SAE-precise
const OVER_REV_RPM: u32 = 4200;
const CONFIRM_AFTER_OCCURRENCES: u32 = 3;

/// Motor controller ECU: an intermittent fault that only trips under
/// simulated load, and only matures from pending to confirmed after
/// recurring a few times — exercising real UDS pending/confirmed status
/// bits instead of a single on/off DTC.
pub struct MotorEcu {
    session: u8,
    pub rpm: u32,
    occurrences: u32,
    dtc_status: u8,
    tick_count: u64,
}

impl MotorEcu {
    pub fn new() -> Self {
        Self {
            session: uds::SESSION_DEFAULT,
            rpm: 3000,
            occurrences: 0,
            dtc_status: 0,
            tick_count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.rpm = 3000 + ((self.tick_count % 7) as u32) * 300;
        if self.rpm > OVER_REV_RPM {
            self.occurrences += 1;
            self.dtc_status |= uds::DTC_STATUS_PENDING | uds::DTC_STATUS_TEST_FAILED;
            if self.occurrences >= CONFIRM_AFTER_OCCURRENCES {
                self.dtc_status |= uds::DTC_STATUS_CONFIRMED;
            }
        }
    }

    pub fn handle_request(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sid) = req.first() else {
            return uds::negative_response(0x00, uds::NRC_SERVICE_NOT_SUPPORTED);
        };
        match sid {
            uds::SID_DIAGNOSTIC_SESSION_CONTROL => {
                let Some(&sub) = req.get(1) else {
                    return uds::negative_response(sid, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
                };
                if sub != uds::SESSION_DEFAULT && sub != uds::SESSION_EXTENDED {
                    return uds::negative_response(sid, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
                }
                self.session = sub;
                uds::build_session_control_response(sub)
            }
            uds::SID_CLEAR_DIAGNOSTIC_INFORMATION => {
                self.dtc_status = 0;
                self.occurrences = 0;
                vec![sid + 0x40]
            }
            uds::SID_READ_DTC_INFORMATION => {
                let Some(&sub) = req.get(1) else {
                    return uds::negative_response(sid, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
                };
                if sub != uds::REPORT_DTC_BY_STATUS_MASK {
                    return uds::negative_response(sid, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
                }
                let dtcs = if self.dtc_status != 0 {
                    vec![Dtc { code: DTC_OVER_REV, status: self.dtc_status }]
                } else {
                    vec![]
                };
                uds::build_dtc_scan_response(&dtcs)
            }
            uds::SID_READ_DATA_BY_IDENTIFIER => {
                let (Some(&hi), Some(&lo)) = (req.get(1), req.get(2)) else {
                    return uds::negative_response(sid, uds::NRC_REQUEST_OUT_OF_RANGE);
                };
                let did = ((hi as u16) << 8) | lo as u16;
                let data: Vec<u8> = match did {
                    0xF190 => b"VDTSIM0002".to_vec(),
                    0x1010 => vec![(self.rpm >> 8) as u8, (self.rpm & 0xFF) as u8],
                    _ => return uds::negative_response(sid, uds::NRC_REQUEST_OUT_OF_RANGE),
                };
                let mut resp = vec![sid + 0x40, hi, lo];
                resp.extend_from_slice(&data);
                resp
            }
            _ => uds::negative_response(sid, uds::NRC_SERVICE_NOT_SUPPORTED),
        }
    }
}

impl Default for MotorEcu {
    fn default() -> Self {
        Self::new()
    }
}
