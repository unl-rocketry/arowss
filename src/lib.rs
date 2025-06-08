pub mod runcam;
pub mod utils;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize, Serializer};
use utils::crc8;

/// A packet sent from the rocket to the ground station.
///
/// Contains information about position and internal payload conditions.
/// Most fields are optional, as it is possible for any part of the payload
/// to be not functioning while still grabbing some data from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryPacket {
    /// Full GPS telemetry information
    pub gps: Option<GpsInfo>,

    /// Environmental information
    pub environmental_info: Option<EnvironmentalInfo>,

    /// Arbitrary information to transfer to the ground
    pub info: VecDeque<String>,
}

impl TelemetryPacket {
    pub fn vec_crc(&self) -> (Vec<u8>, u8) {
        let self_json = serde_json::to_vec(self).unwrap();
        let crc = crc8(&self_json);

        (self_json, crc)
    }

    /// Calculate CRC from json serialized packet data.
    pub fn crc(&self) -> u8 {
        let self_json = serde_json::to_vec(self).unwrap();
        crc8(&self_json)
    }

    /// Validate the packet against its CRC.
    #[must_use]
    pub fn validate(&self, crc: u8) -> bool {
        let self_json = serde_json::to_string(self).unwrap();
        let new_crc = crc8(self_json.as_bytes());

        // If they aren't equal, the data is invalid!
        new_crc == crc
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename = "env")]
pub struct EnvironmentalInfo {
    /// Pressure of the inside of the payload
    #[serde(serialize_with = "truncate_float")]
    #[serde(rename = "pres")]
    pub pressure: f64,
    /// Temperature of the inside of the payload
    #[serde(serialize_with = "truncate_float")]
    #[serde(rename = "temp")]
    pub temperature: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GpsInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f32,
}

fn truncate_float<S: Serializer>(float: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{float:.2}"))
}
