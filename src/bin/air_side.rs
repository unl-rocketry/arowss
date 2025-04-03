use std::{thread::sleep, time::Duration};
use nmea::{Nmea, SentenceType, SENTENCE_MAX_LEN};
use serde::{Deserialize, Serialize};
use chrono::NaiveTime;
use WAIFU::TelemetryPacket;

fn main() {
    let mut gps_port = serialport::new("/dev/ttyACM0", 115200)
        .open()
        .unwrap();
    gps_port.set_timeout(Duration::from_millis(50)).unwrap();

    let mut rfd_port = serialport::new("/dev/ttyUSB0", 57600)
        .open()
        .unwrap();
    rfd_port.set_timeout(Duration::from_millis(50)).unwrap();

    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();
    let mut new_string = String::new();

    loop {
        sleep(Duration::from_millis(500));

        match gps_port.read_to_string(&mut new_string) {
            Ok(_) => (),
            Err(_) => (),
        }

        for line in new_string.lines()
            .filter(|l| !l.is_empty())
            .filter(|l| l.len() < SENTENCE_MAX_LEN)
        {
            dbg!(&line);
            let _ = nmea_parser.parse_for_fix(&line);
        }

        let packet = TelemetryPacket {
            timestamp: nmea_parser.fix_timestamp(),
            latitude: nmea_parser.latitude(),
            longitude: nmea_parser.longitude(),
            altitude: nmea_parser.altitude(),
            pressure: Some(0.0),
            temperature: Some(0.0),
        };

        dbg!(&packet);

        let packet_json = serde_json::to_string(&packet).unwrap();

        rfd_port.write_all(&packet_json.as_bytes()).unwrap();
        rfd_port.write_all(b"\n").unwrap();

        new_string.clear();
    }
}
