mod runcam;

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetryPacket {
    pub timestamp: Option<NaiveTime>,
    pub gps_point: Option<GPSPoint>,
    /// Pressure of the inside of the payload
    pub pressure: Option<f64>,
    /// Temperature of the inside of the payload
    pub temperature: Option<f64>,
    /// The voltage of the main battery
    pub voltage: Option<f64>,
    /// Current being drawn by the main battery
    pub current: Option<f64>,
}

/// A point in 3D space around the Earth expressed in GPS.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GPSPoint {
    /// Latitude in decimal degrees
    pub latitude: f64,
    /// Longitude in decimal degrees
    pub longitude: f64,
    /// Altitude in meters
    pub altitude: f32,
}

impl GPSPoint {
    pub fn new(latitude: f64, longitude: f64, altitude: f32) -> Self {
        Self { latitude, longitude, altitude }
    }
}
