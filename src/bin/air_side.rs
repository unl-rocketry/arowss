use arowss::{EnvironmentalInfo, GpsInfo, PowerInfo, TelemetryPacket};
use bmp388::{BMP388, PowerControl};
use ina219::SyncIna219;
use linux_embedded_hal::I2cdev;
use nmea::{Nmea, SentenceType};
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    sync::Mutex,
    time::{Instant, sleep, sleep_until},
};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    let mut rfd_port = tokio_serial::new("/dev/ttyUSB0", 57600)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    // Spawn GPS task
    let gps_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let gps_data = gps_data.clone();
        async move { gps_loop(gps_data).await }
    });

    // Spawn INA task
    let ina_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let ina_data = ina_data.clone();
        async move { ina_loop(ina_data).await }
    });

    // Spawn BMP task
    let bmp_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let bmp_data = bmp_data.clone();
        async move { bmp_loop(bmp_data).await }
    });

    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    //
    // Every packet begins with a CRC as a decimal number, followed by a space
    // followed by the JSON data, and terminated by a newline (`\n`).
    loop {
        let timeout = Instant::now() + Duration::from_millis(250);

        // Construct a packet from the data
        let packet = TelemetryPacket {
            gps: *gps_data.lock().await,
            power_info: *ina_data.lock().await,
            environmental_info: *bmp_data.lock().await,
        };

        // Calculate the CRC of the packet based on its data.
        let packet_crc = packet.crc();

        // Write the data out
        rfd_port.write_all(packet_crc.to_string().as_bytes()).await.unwrap();
        rfd_port.write_u8(b' ').await.unwrap();
        let json_vec = serde_json::to_vec(&packet).unwrap();
        rfd_port.write_all(&json_vec).await.unwrap();

        // If there is any time left over, sleep
        sleep_until(timeout).await;
    }
}

/// Function to read the GPS module.
async fn gps_loop(data: Arc<Mutex<Option<GpsInfo>>>) -> ! {
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
        let byte_count = gps_port.read_buf(&mut buffer).await.unwrap();

        if byte_count == 0 {
            continue;
        }

        let gps_string = String::from_utf8_lossy(&buffer[..byte_count]);
        let line_count = gps_string.lines().count();

        // If the last line does not end with `\r\n` then it needs more data
        let last_line_incomplete = !gps_string.ends_with("\r\n");

        // Loop over all the lines in the string, skipping the last one if it's
        // incomplete
        for line in gps_string
            .lines()
            .take(line_count - last_line_incomplete as usize)
            .filter(|l| !l.is_empty())
            .filter(|l| l.starts_with("$"))
        {
            let _ = nmea_parser.parse_for_fix(line);
        }

        if nmea_parser.latitude.is_some()
            && nmea_parser.longitude.is_some()
            && nmea_parser.altitude.is_some()
        {
            *data.lock().await = Some(GpsInfo {
                latitude: nmea_parser.latitude.unwrap(),
                longitude: nmea_parser.longitude.unwrap(),
                altitude: nmea_parser.altitude.unwrap(),
            });
        }

        // Put the last line at the beginning of the new buffer
        if last_line_incomplete {
            let last_line = gps_string.lines().last().unwrap().to_string();
            buffer.clear();
            buffer.copy_from_slice(last_line.as_bytes());
        } else {
            buffer.clear();
        }
    }
}

/// Function to read the INA current sensor.
async fn ina_loop(data: Arc<Mutex<Option<PowerInfo>>>) -> ! {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut ina = SyncIna219::new(i2c, ina219::address::Address::from_byte(0x40).unwrap()).unwrap();

    loop {
        sleep(Duration::from_millis(250)).await;

        *data.lock().await = Some(PowerInfo {
            voltage: ina.bus_voltage().unwrap().voltage_mv(),
            current: ina.current_raw().unwrap().0,
        });
    }
}

async fn bmp_loop(data: Arc<Mutex<Option<EnvironmentalInfo>>>) -> ! {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut delay = linux_embedded_hal::Delay;
    let mut bmp = BMP388::new_blocking(i2c, bmp388::Addr::Secondary as u8, &mut delay).unwrap();

    // set power control to normal
    bmp.set_power_control(PowerControl::normal()).unwrap();

    // Set up measurement settings
    bmp.set_oversampling(bmp388::config::OversamplingConfig {
        osr_pressure: bmp388::Oversampling::x8,
        osr_temperature: bmp388::Oversampling::x1
    }).unwrap();
    bmp.set_filter(bmp388::Filter::c3).unwrap();
    bmp.set_sampling_rate(bmp388::SamplingRate::ms20).unwrap();

    loop {
        sleep(Duration::from_millis(50)).await;

        let sensor_data = bmp.sensor_values().unwrap();

        *data.lock().await = Some(EnvironmentalInfo {
            pressure: sensor_data.pressure,
            temperature: sensor_data.temperature,
        });
    }
}
