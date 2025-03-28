#![allow(non_snake_case)]

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetryPacket {
    pub timestamp: Option<NaiveTime>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f32>,
    pub pressure: Option<f64>,
    pub temperature: Option<f64>
}
