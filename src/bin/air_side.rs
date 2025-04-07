use std::{thread::sleep, time::Duration};
use nmea::{Nmea, SentenceType, SENTENCE_MAX_LEN};
use arowss::TelemetryPacket;

fn main() {
    let mut rfd_port = serialport::new("/dev/ttyUSB0", 57600).open().unwrap();
    rfd_port.set_timeout(Duration::from_millis(50)).unwrap();

    let (gps_tx, mut gps_rx) = tokio::sync::watch::channel(0);
    tokio::spawn(async move { gps_loop() });

    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    loop {
        sleep(Duration::from_millis(250));

        let mut packet = TelemetryPacket {
            gps: None,
            pressure: None,
            temperature: None,
            voltage: None,
            current: None,
            ..Default::default()
        };

        packet.insert_crc();

        dbg!(&packet);

        serde_json::to_writer(&mut rfd_port, &packet).unwrap();
        rfd_port.write_all(b"\n").unwrap();
    }
}

async fn gps_loop() -> ! {
    // Set up the GPS serial port. This must utilize the proper port on the
    // raspberry pi.
    let mut gps_port = serialport::new("/dev/ttyACM0", 115200)
        .open()
        .unwrap();
    gps_port.set_timeout(Duration::from_millis(50)).unwrap();

    // Set up and configure the NMEA parser.
    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();
    let mut new_string = String::new();

    loop {
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
    }
}
