use std::{thread::sleep, time::Duration};

use arowss::TelemetryPacket;

fn main() {
    let mut rfd_port = serialport::new("/dev/ttyUSB0", 57600)
        .open()
        .unwrap();

    rfd_port.set_timeout(Duration::from_millis(50)).unwrap();

    loop {
        sleep(Duration::from_millis(500));

        let mut packet_string = String::new();
        rfd_port.read_to_string(&mut packet_string).unwrap_or_default() ;

        let packet: TelemetryPacket = match serde_json::from_str(&packet_string) {
            Ok(p) => p,
            Err(_) => continue,
        };

        dbg!(&packet);
    }
}
