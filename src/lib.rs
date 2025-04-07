mod runcam;

use nmea::Nmea;
use serde::{Deserialize, Serialize};

pub const CRC_CKSUM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

/// A packet sent from the rocket to the ground station. Contains information
/// about position and internal payload conditions. Most fields are optional,
/// as it is possible for any part of the payload to be not functioning while
/// still grabbing some data from it.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetryPacket {
    /// Full GPS telemetry information
    pub gps: Option<Nmea>,

    /// Pressure of the inside of the payload
    pub pressure: Option<f64>,
    /// Temperature of the inside of the payload
    pub temperature: Option<f64>,

    /// The voltage of the main battery
    pub voltage: Option<f64>,
    /// Current being drawn by the main battery
    pub current: Option<f64>,

    /// CRC of the contents of this packet, not including this field, as
    /// json.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crc: Option<u32>,
}

impl TelemetryPacket {
    pub fn insert_crc(&mut self) {
        self.crc = None;

        let self_json = serde_json::to_string(self).unwrap();
        let crc = CRC_CKSUM.checksum(self_json.as_bytes());

        self.crc = Some(crc);
    }
}
