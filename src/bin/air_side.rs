use std::{sync::Arc, thread::sleep, time::Duration};
use linux_embedded_hal::I2cdev;
use nmea::{Nmea, SentenceType};
use arowss::TelemetryPacket;
use tokio::{io::{AsyncReadExt as _, AsyncWriteExt as _}, sync::RwLock};
use ina219::{address::Address, SyncIna219};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    let mut rfd_port = tokio_serial::new("/dev/ttyUSB0", 57600)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    // Spawn GPS task
    let gps_data = Arc::new(RwLock::new(None));
    tokio::spawn({
        let gps_data = gps_data.clone();
        async move { gps_loop(gps_data.clone()).await }
    });

    // Spawn INA task
    tokio::spawn({
        async move { ina_loop().await }
    });

    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    //
    // Every packet is a single line of JSON, followed by a newline, followed
    // by a CRC, followed by another newline. The CRC validates the JSON.
    loop {
        sleep(Duration::from_millis(250));

        let gps = gps_data.try_read().map_or(None, |e| e.clone().or(None));

        let packet = TelemetryPacket {
            gps,
            pressure: None,
            temperature: None,
            voltage: None,
            current: None,
        };

        let packet_crc = packet.crc();

        serde_json::to_writer(&mut rfd_port, &packet).unwrap();
        rfd_port.write_all(b"\n").await.unwrap();
        rfd_port.write_all(packet_crc.to_string().as_bytes()).await.unwrap();
        rfd_port.write_all(b"\n").await.unwrap();
    }
}

/// Function to read the GPS module.
async fn gps_loop(data: Arc<RwLock<Option<Nmea>>>) -> ! {
    // Set up the GPS serial port. This must utilize the proper port on the
    // raspberry pi.
    let mut gps_port = tokio_serial::new("/dev/ttyACM0", 115200)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    // Set up and configure the NMEA parser.
    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();

    let mut buffer = Vec::new();

    loop {
        let _byte_count = gps_port.read(&mut buffer).await.unwrap();

        if buffer.is_empty() {
            continue;
        }

        let new_string = String::from_utf8_lossy(&buffer);

        for line in new_string.lines()
            .filter(|l| !l.is_empty())
            .filter(|l| l.starts_with("$"))
        {
            // Handle unfinished lines
            if !line.ends_with("\r\n") {

            }

            let _ = nmea_parser.parse_for_fix(&line);
        }

        buffer.clear();

        *data.write().await = Some(nmea_parser.clone());
    }
}

async fn ina_loop() -> ! {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut ina = SyncIna219::new(i2c, Address::from_byte(0x40).unwrap()).unwrap();

    loop {
        std::thread::sleep(ina.configuration().unwrap().conversion_time().unwrap());

        println!("Bus Voltage: {}", ina.bus_voltage().unwrap());
        println!("Shunt Voltage: {}", ina.shunt_voltage().unwrap());
        println!("Current: {:?}", ina.current_raw().unwrap());
        println!("Power: {:?}", ina.power_raw().unwrap());
    }
}
