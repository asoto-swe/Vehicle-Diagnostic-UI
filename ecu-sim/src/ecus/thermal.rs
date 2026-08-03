use crate::uds::{self, Dtc};

const DTC_COOLANT_OVER_TEMP: u32 = 0x02_1800; // illustrative, ~P0218
const PUMP_FAIL_THRESHOLD_C: f32 = 75.0;
const ROUTINE_COOLING_PUMP_TEST: u16 = 0xFF00;
// Deliberately trivial "security algorithm" for a demo ECU, not real crypto.
const SECURITY_KEY_XOR: u32 = 0xA5A5_A5A5;

/// Thermal/cooling ECU: the root cause of a cascading fault (a failed
/// coolant pump raises battery temp elsewhere on the bus — see
/// fault_engine::spawn_bms) and, once the pump has been down a while, the
/// coolant sensor itself starts reporting a suspicious flat value instead
/// of tracking reality (a "plausibility" fault, not a threshold fault).
///
/// Also the only ECU wired up with Security Access + Routine Control, since
/// a cooling-pump actuator test is a natural fit for "run this routine."
pub struct ThermalEcu {
    session: u8,
    pub coolant_temp_c: f32,
    pub pump_ok: bool,
    dtc_status: u8,
    security_unlocked: bool,
    pending_seed: Option<u32>,
    tick_count: u64,
    sensor_stuck: bool,
}

impl ThermalEcu {
    pub fn new() -> Self {
        Self {
            session: uds::SESSION_DEFAULT,
            coolant_temp_c: 55.0,
            pump_ok: true,
            dtc_status: 0,
            security_unlocked: false,
            pending_seed: None,
            tick_count: 0,
            sensor_stuck: false,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.coolant_temp_c += if self.pump_ok { 0.3 } else { 1.2 };

        if self.pump_ok && self.coolant_temp_c > PUMP_FAIL_THRESHOLD_C {
            self.pump_ok = false;
            self.dtc_status = uds::DTC_STATUS_CONFIRMED | uds::DTC_STATUS_TEST_FAILED;
        }

        if !self.pump_ok && self.tick_count % 40 == 20 {
            self.sensor_stuck = !self.sensor_stuck;
        }
    }

    pub fn status_broadcast_payload(&self) -> [u8; 2] {
        [self.pump_ok as u8, self.coolant_temp_c.clamp(0.0, 255.0) as u8]
    }

    pub fn handle_request(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sid) = req.first() else {
            return uds::negative_response(0x00, uds::NRC_SERVICE_NOT_SUPPORTED);
        };
        match sid {
            uds::SID_DIAGNOSTIC_SESSION_CONTROL => self.session_control(req),
            uds::SID_CLEAR_DIAGNOSTIC_INFORMATION => {
                self.dtc_status = 0;
                vec![sid + 0x40]
            }
            uds::SID_READ_DTC_INFORMATION => self.read_dtc_information(req),
            uds::SID_READ_DATA_BY_IDENTIFIER => self.read_data_by_identifier(req),
            uds::SID_SECURITY_ACCESS => self.security_access(req),
            uds::SID_ROUTINE_CONTROL => self.routine_control(req),
            _ => uds::negative_response(sid, uds::NRC_SERVICE_NOT_SUPPORTED),
        }
    }

    fn session_control(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sub) = req.get(1) else {
            return uds::negative_response(uds::SID_DIAGNOSTIC_SESSION_CONTROL, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        if sub != uds::SESSION_DEFAULT && sub != uds::SESSION_EXTENDED {
            return uds::negative_response(uds::SID_DIAGNOSTIC_SESSION_CONTROL, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        }
        self.session = sub;
        uds::build_session_control_response(sub)
    }

    fn read_dtc_information(&self, req: &[u8]) -> Vec<u8> {
        let Some(&sub) = req.get(1) else {
            return uds::negative_response(uds::SID_READ_DTC_INFORMATION, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        if sub != uds::REPORT_DTC_BY_STATUS_MASK {
            return uds::negative_response(uds::SID_READ_DTC_INFORMATION, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        }
        let dtcs = if self.dtc_status != 0 {
            vec![Dtc { code: DTC_COOLANT_OVER_TEMP, status: self.dtc_status }]
        } else {
            vec![]
        };
        uds::build_dtc_scan_response(&dtcs)
    }

    fn read_data_by_identifier(&self, req: &[u8]) -> Vec<u8> {
        let (Some(&hi), Some(&lo)) = (req.get(1), req.get(2)) else {
            return uds::negative_response(uds::SID_READ_DATA_BY_IDENTIFIER, uds::NRC_REQUEST_OUT_OF_RANGE);
        };
        let did = ((hi as u16) << 8) | lo as u16;
        let data: Vec<u8> = match did {
            0xF190 => b"VDTSIM0003".to_vec(),
            // Plausibility fault: once "stuck," this reports a flat 0C
            // instead of the real (rising) temperature.
            0x1002 => vec![if self.sensor_stuck { 0 } else { self.coolant_temp_c.clamp(0.0, 255.0) as u8 }],
            0x1003 => vec![self.pump_ok as u8],
            _ => return uds::negative_response(uds::SID_READ_DATA_BY_IDENTIFIER, uds::NRC_REQUEST_OUT_OF_RANGE),
        };
        let mut resp = vec![uds::SID_READ_DATA_BY_IDENTIFIER + 0x40, hi, lo];
        resp.extend_from_slice(&data);
        resp
    }

    fn security_access(&mut self, req: &[u8]) -> Vec<u8> {
        let Some(&sub) = req.get(1) else {
            return uds::negative_response(uds::SID_SECURITY_ACCESS, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        match sub {
            uds::SECURITY_REQUEST_SEED => {
                let seed = 0x1234_5678_u32 ^ (self.tick_count as u32);
                self.pending_seed = Some(seed);
                let mut resp = vec![uds::SID_SECURITY_ACCESS + 0x40, sub];
                resp.extend_from_slice(&seed.to_be_bytes());
                resp
            }
            uds::SECURITY_SEND_KEY => {
                let Some(seed) = self.pending_seed.take() else {
                    return uds::negative_response(uds::SID_SECURITY_ACCESS, uds::NRC_REQUEST_OUT_OF_RANGE);
                };
                if req.len() < 6 {
                    return uds::negative_response(uds::SID_SECURITY_ACCESS, uds::NRC_INVALID_KEY);
                }
                let key = u32::from_be_bytes([req[2], req[3], req[4], req[5]]);
                if key == seed ^ SECURITY_KEY_XOR {
                    self.security_unlocked = true;
                    vec![uds::SID_SECURITY_ACCESS + 0x40, sub]
                } else {
                    uds::negative_response(uds::SID_SECURITY_ACCESS, uds::NRC_INVALID_KEY)
                }
            }
            _ => uds::negative_response(uds::SID_SECURITY_ACCESS, uds::NRC_SUBFUNCTION_NOT_SUPPORTED),
        }
    }

    fn routine_control(&mut self, req: &[u8]) -> Vec<u8> {
        if !self.security_unlocked {
            return uds::negative_response(uds::SID_ROUTINE_CONTROL, uds::NRC_SECURITY_ACCESS_DENIED);
        }
        let (Some(&sub), Some(&hi), Some(&lo)) = (req.get(1), req.get(2), req.get(3)) else {
            return uds::negative_response(uds::SID_ROUTINE_CONTROL, uds::NRC_SUBFUNCTION_NOT_SUPPORTED);
        };
        let routine_id = ((hi as u16) << 8) | lo as u16;
        if routine_id != ROUTINE_COOLING_PUMP_TEST {
            return uds::negative_response(uds::SID_ROUTINE_CONTROL, uds::NRC_REQUEST_OUT_OF_RANGE);
        }
        match sub {
            uds::ROUTINE_START | uds::ROUTINE_REQUEST_RESULTS => {
                let result: u8 = if self.pump_ok { 0x00 } else { 0x01 }; // 0=passed, 1=failed
                vec![uds::SID_ROUTINE_CONTROL + 0x40, sub, hi, lo, result]
            }
            uds::ROUTINE_STOP => vec![uds::SID_ROUTINE_CONTROL + 0x40, sub, hi, lo],
            _ => uds::negative_response(uds::SID_ROUTINE_CONTROL, uds::NRC_SUBFUNCTION_NOT_SUPPORTED),
        }
    }
}

impl Default for ThermalEcu {
    fn default() -> Self {
        Self::new()
    }
}
