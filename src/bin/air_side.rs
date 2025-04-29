use arowss::{EnvironmentalInfo, GpsInfo, PowerInfo, TelemetryPacket, utils::crc8};
use bmp388::{BMP388, PowerControl};
use ina219::SyncIna219;
use linux_embedded_hal::I2cdev;
use tracing::{error, info, Level};
use nmea::{Nmea, SentenceType};
use num_derive::{FromPrimitive, ToPrimitive};
use rppal::gpio::Gpio;
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    join,
    sync::Mutex,
    time::{Instant, sleep, sleep_until},
};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::fmt()
        .with_max_level(Level::INFO)
        .with_thread_ids(true)
        .init();

    // Spawn and wait on the tasks until they finish, which they should never
    let send = tokio::spawn(sending_loop());
    let recv = tokio::spawn(command_loop());

    info!("Waiting on tasks...");
    #[allow(unused_must_use)]
    {
        join!(send, recv);
    }
}

async fn sending_loop() {
    info!("Initalized telemetry sending");

    let Ok(mut rfd_send) = tokio_serial::new("/dev/ttyUSB0", 57600)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .data_bits(tokio_serial::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .inspect_err(|e| error!("Could not open RFD for sending: {e}"))
    else {
        return
    };

    rfd_send.set_exclusive(false).unwrap();

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
            .await
            .unwrap();
        rfd_send.write_u8(b' ').await.unwrap();
        let json_vec = serde_json::to_vec(&packet).unwrap();
        rfd_send.write_all(&json_vec).await.unwrap();
        rfd_send.write_u8(b'\n').await.unwrap();

        // If there is any time left over, sleep
        sleep_until(timeout).await;
    }
}

const HIGH_POWER_RELAY_PIN_NUM: u8 = 26;

async fn command_loop() {
    info!("Initalized command receiving");

    let Ok(mut rfd_recv) = tokio_serial::new("/dev/ttyUSB0", 57600)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .data_bits(tokio_serial::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .inspect_err(|e| error!("Could not open RFD for commands: {e}"))
    else {
        return
    };

    rfd_recv.set_exclusive(false).unwrap();

    let gpio = Gpio::new().unwrap();
    let relay_pin = gpio.get(HIGH_POWER_RELAY_PIN_NUM)
        .unwrap()
        .into_output_low();

    let mut buf = Vec::new();
    loop {
        let new_byte = rfd_recv.read_u8().await.unwrap();

        buf.push(new_byte);

        if buf.len() < 3 {
            continue;
        } else if buf.len() >= 3 && buf.last() != Some(&b'\n') {
            buf.clear();
            continue;
        }

        if buf.len() == 3 && buf.last() == Some(&b'\n') {
            let data = buf[0];
            let check = buf[1];

            let new_cksum = crc8(&[data]);

            if check != new_cksum {
                error!(
                    "Checksums do not match ({} != {}), discarding packet",
                    check,
                    new_cksum
                );
                continue;
            }

            // Do the command parsing logic here....
        }
    }
}

/// Function to read the Ublox ZED-F9P GPS module.
async fn gps_loop(data: Arc<Mutex<Option<GpsInfo>>>) -> ! {
    // Set up the GPS serial port. This must utilize the proper port on the
    // raspberry pi.
    let mut gps_port = tokio_serial::new("/dev/ttyACM0", 115200)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    // Set up and configure the NMEA parser.
    let mut nmea_parser = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();

    let mut buffer = [0u8; 4096];

    loop {
        let byte_count = gps_port.read(&mut buffer).await.unwrap();

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

/// Commands which the air side code must respond to from the ground.
#[derive(FromPrimitive, ToPrimitive)]
#[repr(u8)]
pub enum Commands {
    /// Enable the High Power components via the relay
    EnableHighPower,
    /// Disable the High Power components via the relay
    DisableHighPower,

    ExampleCommand3,
    ExampleCommand4,
}
