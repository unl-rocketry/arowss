pub mod runcam;
pub mod utils;

use bon::Builder;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use utils::crc8;

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
    pub version: u8,

    /// A squence number from 0-255 which allows detection of missed packets.
    #[serde(rename = "seq")]
    pub sequence_number: u8,

    /// Full GPS telemetry information
    pub gps: Option<GpsInfo>,

    /// Environmental information
    #[serde(rename = "env")]
    pub environmental_info: Option<EnvironmentalInfo>,

    /// Battery related information
    #[serde(rename = "pwr")]
    pub power_info: Option<PowerInfo>,
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
pub struct PowerInfo {
    /// The voltage of the main battery
    #[serde(rename = "volt")]
    pub voltage: u16,
    /// Current being drawn by all components from the main battery
    #[serde(rename = "curr")]
    pub current: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnvironmentalInfo {
    /// Pressure of the inside of the payload
    #[serde(rename = "pres")]
    pub pressure: f64,
    /// Temperature of the inside of the payload
    #[serde(rename = "temp")]
    pub temperature: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GpsInfo {
    /// DateTime of the latest fix
    pub datetime: NaiveTime,

    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f32>,
}
