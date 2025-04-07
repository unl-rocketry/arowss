mod runcam;

use nmea::Nmea;
use serde::{Deserialize, Serialize};

pub const CRC_CKSUM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

/// A packet sent from the rocket to the ground station. Contains information
/// about position and internal payload conditions. Most fields are optional,
/// as it is possible for any part of the payload to be not functioning while
/// still grabbing some data from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryPacket {
    /// Full GPS telemetry information
    pub gps: Option<Nmea>,

    /// Pressure of the inside of the payload
    pub pressure: Option<f64>,
    /// Temperature of the inside of the payload
    pub temperature: Option<f64>,

    /// The voltage of the main battery
    pub voltage: Option<f64>,
    /// Current being drawn by all components from the main battery
    pub current: Option<f64>,
}

impl TelemetryPacket {
    /// Calculate CRC from json serialized packet data.
    pub fn crc(&self) -> u32 {
        let self_json = serde_json::to_string(self).unwrap();
        CRC_CKSUM.checksum(self_json.as_bytes())
    }

    /// Validate the packet against its CRC.
    pub fn validate(&self, crc: u32) -> bool {
        let self_json = serde_json::to_string(self).unwrap();
        let new_crc = CRC_CKSUM.checksum(self_json.as_bytes());

        // If they aren't equal, the data is invalid!
        new_crc == crc
    }
}
