use crate::uds::{self, Dtc};

const DTC_BATTERY_OVER_TEMP: u32 = 0x0A_8000; // illustrative, ~P0A80
const OVER_TEMP_THRESHOLD_C: f32 = 60.0;

/// Battery management ECU: the straightforward fault case — a cell
/// temperature that drifts past a fixed threshold sets a confirmed DTC.
pub struct BmsEcu {
    session: u8,
    pub battery_temp_c: f32,
    pub battery_soc: f32,
    dtc_status: u8,
}

impl BmsEcu {
    pub fn new() -> Self {
        Self {
            session: uds::SESSION_DEFAULT,
            battery_temp_c: 38.0,
            battery_soc: 88.0,
            dtc_status: 0,
        }
    }

    pub fn tick(&mut self) {
        self.battery_temp_c += 0.05;
        if self.battery_temp_c > OVER_TEMP_THRESHOLD_C {
            self.dtc_status = uds::DTC_STATUS_CONFIRMED | uds::DTC_STATUS_TEST_FAILED;
        }
    }

    /// Called by the fault engine when the thermal ECU's broadcast shows a
    /// failed coolant pump: heat propagates into the battery pack over
    /// time, so a thermal-system fault can surface as a battery DTC too.
    pub fn apply_coolant_influence(&mut self, coolant_temp_c: f32, pump_ok: bool) {
        if !pump_ok && coolant_temp_c > 70.0 {
            self.battery_temp_c += 0.8;
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
                    vec![Dtc { code: DTC_BATTERY_OVER_TEMP, status: self.dtc_status }]
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
                    0xF190 => b"VDTSIM0001".to_vec(),
                    0x1000 => vec![self.battery_temp_c.clamp(0.0, 255.0) as u8],
                    0x1001 => vec![self.battery_soc.clamp(0.0, 255.0) as u8],
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

impl Default for BmsEcu {
    fn default() -> Self {
        Self::new()
    }
}
