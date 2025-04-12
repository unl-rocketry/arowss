use std::time::Duration;
use bmp388::{PowerControl, BMP388};
use linux_embedded_hal::I2cdev;
use nmea::{Nmea, SentenceType};
use arowss::{EnvironmentalInfo, PowerInfo, TelemetryPacket};
use tokio::{io::{AsyncReadExt as _, AsyncWriteExt as _}, sync::watch::{self, Sender}, time::{sleep, sleep_until, Instant}};
use ina219::{address::Address, SyncIna219};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    let mut rfd_port = tokio_serial::new("/dev/ttyUSB0", 57600)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    // Spawn GPS task
    let (gps_tx, gps_rx) = watch::channel(None);
    tokio::spawn(async move { gps_loop(gps_tx).await });

    // Spawn INA task
    let (ina_tx, ina_rx) = watch::channel(None);
    tokio::spawn(async move { ina_loop(ina_tx).await });

    // Spawn BMP task
    let (bmp_tx, bmp_rx) = watch::channel(None);
    tokio::spawn(async move { bmp_loop(bmp_tx).await });


    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    //
    // Every packet is a single line of JSON, followed by a newline, followed
    // by a CRC, followed by another newline. The CRC validates the JSON.
    loop {
        let timeout = Instant::now() + Duration::from_millis(250);

        // Construct a packet from the data
        let packet = TelemetryPacket {
            gps: gps_rx.borrow().clone(),
            power_info: *ina_rx.borrow(),
            environmental_info: *bmp_rx.borrow(),
        };

        // Calculate the CRC of the packet based on its data.
        let packet_crc = packet.crc();

        // Write the data out
        serde_json::to_writer(&mut rfd_port, &packet).unwrap();
        rfd_port.write_all(b"\n").await.unwrap();
        rfd_port.write_all(packet_crc.to_string().as_bytes()).await.unwrap();
        rfd_port.write_all(b"\n").await.unwrap();

        // If there is any time left over, sleep
        sleep_until(timeout).await;
    }
}

/// Function to read the GPS module.
async fn gps_loop(data: Sender<Option<Nmea>>) -> ! {
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
            // Handle unfinished lines eventually?
            if !line.ends_with("\r\n") {
                continue;
            }

            let _ = nmea_parser.parse_for_fix(line);
        }

        buffer.clear();

        data.send(Some(nmea_parser.clone())).unwrap();
    }
}

/// Function to read the INA current sensor.
async fn ina_loop(data: Sender<Option<PowerInfo>>) -> ! {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut ina = SyncIna219::new(i2c, Address::from_byte(0x40).unwrap()).unwrap();

    loop {
        sleep(ina.configuration().unwrap().conversion_time().unwrap()).await;

        data.send(Some(PowerInfo {
            voltage: ina.bus_voltage().unwrap().voltage_mv(),
            current: ina.current_raw().unwrap().0,
        })).unwrap();
    }
}

async fn bmp_loop(data: Sender<Option<EnvironmentalInfo>>) -> ! {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut delay = linux_embedded_hal::Delay;
    let mut bmp = BMP388::new_blocking(i2c, bmp388::Addr::Secondary as u8, &mut delay).unwrap();

    // set power control to normal
    bmp.set_power_control(PowerControl::normal()).unwrap();

    loop {
        let sensor_data = bmp.sensor_values().unwrap();

        data.send(Some(EnvironmentalInfo {
            pressure: sensor_data.pressure,
            temperature: sensor_data.temperature,
        })).unwrap();
    }
}
