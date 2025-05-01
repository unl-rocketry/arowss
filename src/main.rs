mod commands;
use commands::parse_command;

use arowss::{EnvironmentalInfo, GpsInfo, PowerInfo, TelemetryPacket, utils::crc8};
use bmp388::{BMP388, PowerControl};
use ina219::SyncIna219;
use linux_embedded_hal::I2cdev;
use tracing::{warn, debug, error, info, instrument, Level};
use nmea::{Nmea, SentenceType};
use rppal::gpio::Gpio;
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    join,
    sync::Mutex,
    time::{Instant, sleep, sleep_until},
};
use serialport::SerialPort;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::fmt()
        .with_max_level(Level::INFO)
        .with_file(false)
        .init();

    let rfd_port = serialport::new("/dev/ttyUSB0", 57600)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .data_bits(serialport::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open()
        .unwrap();

    let rfd_send = rfd_port.try_clone().unwrap();
    let rfd_recv = rfd_port.try_clone().unwrap();

    // Spawn and wait on the tasks until they finish, which they should never
    let send = tokio::spawn(sending_loop(rfd_send));
    let recv = tokio::spawn(command_loop(rfd_recv));

    info!("Waiting on tasks...");
    #[allow(unused_must_use)]
    {
        join!(send, recv);
    }
}

#[instrument(skip_all)]
async fn sending_loop(mut rfd_send: Box<dyn SerialPort>) {
    info!("Initalized telemetry sending");

    // Spawn GPS task
    let gps_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let gps_data = gps_data.clone();
        async move { gps_loop(gps_data).await }
    });
    info!("Spawned GPS task");

    // Spawn INA task
    let ina_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let ina_data = ina_data.clone();
        async move { ina_loop(ina_data).await }
    });
    info!("Spawned INA task");

    // Spawn BMP task
    let bmp_data = Arc::new(Mutex::new(None));
    tokio::spawn({
        let bmp_data = bmp_data.clone();
        async move { bmp_loop(bmp_data).await }
    });
    info!("Spawned BMP task");

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
        rfd_send
            .write_all(packet_crc.to_string().as_bytes())
            .unwrap();
        rfd_send.write_all(b" ").unwrap();
        let json_vec = serde_json::to_vec(&packet).unwrap();
        rfd_send.write_all(&json_vec).unwrap();
        rfd_send.write_all(b"\n").unwrap();

        debug!("Sent {} bytes, checksum 0x{:0X}", json_vec.len(), packet_crc);

        rfd_send.flush().unwrap();

        // If there is any time left over, sleep
        sleep_until(timeout).await;
    }
}

const HIGH_POWER_RELAY_PIN_NUM: u8 = 26;

#[instrument(skip_all)]
async fn command_loop(mut rfd_recv: Box<dyn SerialPort>) {
    info!("Initalized command receiving");

    let gpio = Gpio::new().unwrap();
    let mut relay_pin = gpio.get(HIGH_POWER_RELAY_PIN_NUM)
        .unwrap()
        .into_output_low();

    let mut buf = Vec::new();
    loop {
        let mut byte_buf = [0];
        if rfd_recv.read_exact(&mut byte_buf).is_err() {
            continue;
        }

        buf.push(byte_buf[0]);

        if buf.len() < 3 {
            continue;
        } else if buf.len() >= 3 && buf.last() != Some(&b' ') {
            warn!("Buffer invalid: {:?}", buf);
            buf.clear();
            continue;
        }

        if buf.len() == 3 && buf.last() == Some(&b' ') {
            info!("Got command {:?}", buf);

            let data = buf[0];
            let check = buf[1];

            let new_cksum = crc8(&[data]);

            if check != new_cksum {
                warn!(
                    "Checksums do not match ({} != {}), discarding packet",
                    check,
                    new_cksum
                );
                continue;
            }

            match parse_command(data, &mut relay_pin).await {
                Ok(s) => println!("{}", s),
                Err(e) => println!("ERR: {:?}, {}", e, e),
            }

            buf.clear();
        }
    }
}

/// Function to read the Ublox ZED-F9P GPS module.
#[instrument(skip_all)]
async fn gps_loop(data: Arc<Mutex<Option<GpsInfo>>>) {
    // Set up the GPS serial port. This must utilize the proper port on the
    // raspberry pi.
    let mut gps_port = serialport::new("/dev/ttyACM0", 115200)
        .timeout(Duration::from_millis(50))
        .open()
        .unwrap();

    // Set up and configure the NMEA parser.
    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();

    let mut buffer = [0u8; 4096];

    loop {
        let byte_count = gps_port.read(&mut buffer).unwrap();

        if byte_count == 0 {
            continue;
        }

        let new_string = String::from_utf8_lossy(&buffer[..byte_count]);

        for line in new_string
            .lines()
            .filter(|l| !l.is_empty())
            .filter(|l| l.starts_with("$"))
        {
            let _ = nmea_parser.parse_for_fix(line);
        }

        if nmea_parser.latitude.is_none()
            || nmea_parser.longitude.is_none()
            || nmea_parser.altitude.is_none()
        {
            continue;
        }

        *data.lock().await = Some(GpsInfo {
            latitude: nmea_parser.latitude.unwrap(),
            longitude: nmea_parser.longitude.unwrap(),
            altitude: nmea_parser.altitude.unwrap(),
        });
    }
}

/// Function to read the INA219 current sensor.
#[instrument(skip_all)]
async fn ina_loop(data: Arc<Mutex<Option<PowerInfo>>>) {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let Ok(mut ina) = SyncIna219::new(i2c, ina219::address::Address::from_byte(0x40).unwrap()) else {
        error!("Could not initalize INA219");
        return
    };

    loop {
        sleep(Duration::from_millis(250)).await;

        *data.lock().await = Some(PowerInfo {
            voltage: ina.bus_voltage().unwrap().voltage_mv(),
            current: ina.current_raw().unwrap().0,
        });
    }
}

/// Function to read the BMP388 pressure and temp sensor.
#[instrument(skip_all)]
async fn bmp_loop(data: Arc<Mutex<Option<EnvironmentalInfo>>>) {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut delay = linux_embedded_hal::Delay;
    let Ok(mut bmp) = BMP388::new_blocking(i2c, bmp388::Addr::Secondary as u8, &mut delay) else {
        error!("Could not initalize BMP388");
        return
    };

    // set power control to normal
    bmp.set_power_control(PowerControl::normal()).unwrap();

    // Set up measurement settings
    bmp.set_oversampling(bmp388::config::OversamplingConfig {
        osr_pressure: bmp388::Oversampling::x8,
        osr_temperature: bmp388::Oversampling::x1,
    })
    .unwrap();
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