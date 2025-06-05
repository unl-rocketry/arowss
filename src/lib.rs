pub mod runcam;
pub mod utils;

use bon::Builder;
use serde::{Deserialize, Serialize};
use utils::{crc8, truncate_float};

/// The current version of the packet format.
const PACKET_VERSION: u8 = 2;

/// A packet sent from the rocket to the ground station.
///
/// Contains information about position and internal payload conditions.
/// Most fields are optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[derive(Builder)]
pub struct TelemetryPacket {
    #[builder(skip = PACKET_VERSION)]
    #[serde(rename = "v")]
    pub version: u8,

    /// Full GPS telemetry information
    pub gps: GpsInfo,

    /// Environmental information
    #[serde(rename = "env")]
    pub environmental_info: Option<EnvironmentalInfo>,
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
pub struct EnvironmentalInfo {
    /// Pressure of the inside of the payload
    #[serde(rename = "pres", serialize_with = "truncate_float")]
    pub pressure: f64,
    /// Temperature of the inside of the payload
    #[serde(rename = "temp", serialize_with = "truncate_float")]
    pub temperature: f64,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct GpsInfo {
    /// Number of visible satellites
    pub sats: u8,
    #[serde(rename = "lat")]
    pub latitude: Option<f64>,
    #[serde(rename = "lon")]
    pub longitude: Option<f64>,
    #[serde(rename = "alt")]
    pub altitude: Option<f32>,
}
