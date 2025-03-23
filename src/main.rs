use std::{thread::sleep, time::Duration};
use nmea::{Nmea, SentenceType, SENTENCE_MAX_LEN};
use serde::{Deserialize, Serialize};

fn main() {
    let mut gps_port = serialport::new("/dev/ttyACM0", 115200)
        .open()
        .unwrap();
   
    let mut rfd_port = serialport::new("/dev/ttyAMA4", 57600)
        .open()
        .unwrap();

    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();
    let mut new_string = String::new();

    loop {
        if !gps_port.bytes_to_read().unwrap() > 83 {
            sleep(Duration::from_millis(500));
        }

        match gps_port.read_to_string(&mut new_string) {
            Ok(_) => (),
            Err(e) => eprintln!("{:?}", e),
        }

        for line in new_string.lines()
            .filter(|l| !l.is_empty()) 
            .filter(|l| l.len() < SENTENCE_MAX_LEN)
        {
            let _ = nmea_parser.parse_for_fix(&line);
        }

        let packet = TelemetryPacket {
            latitude: nmea_parser.latitude(),
            longitude: nmea_parser.longitude(),
            altitude: nmea_parser.altitude(),
            pressure: Some(0.0),
            temperature: Some(0.0),
        };

        let packet_json = serde_json::to_string(&packet).unwrap(); 

        rfd_port.write_all(&packet_json.as_bytes()).unwrap(); 

        new_string.clear();
    }
}

#[derive(Serialize, Deserialize)]
struct TelemetryPacket {
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<f32>,
    pressure: Option<f64>,
    temperature: Option<f64>
}