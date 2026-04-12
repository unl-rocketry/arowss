mod commands;
use bmp581::{Bmp581, I2cAddr, types::{DeepDis, Odr, Osr, PowerMode}};
use commands::CommandParser;

use arowss::{utils::crc8, EnvironmentalInfo, GpsInfo, TelemetryPacket};
use linux_embedded_hal::I2cdev;
use tracing::{warn, debug, error, info, instrument, Level};
use nmea::{Nmea, SentenceType};
use rppal::gpio::Gpio;
use std::{cell::RefCell, collections::VecDeque, sync::{Arc, mpsc::{self, Receiver, Sender}}, time::Duration};
use tokio::{join, spawn, sync::watch, time::{self, sleep}};
use serialport::SerialPort;
use std::sync::Mutex;
use bno055::BNO055PowerMode;
use embedded_hal_bus::i2c::MutexDevice;

const RFD_PATH: &str = "/dev/ttyAMA2";
const RFD_BAUD: u32 = 57600;
/// This is the maximum number of bytes that can be sent by the RFD-900 per
/// packet without dropping behind
const MAX_PACKET_BYTES: usize = (RFD_BAUD as usize / 9) / 4;

const GPS_PATH: &str = "/dev/ttyAMA3";
const GPS_BAUD: u32 = 57600;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::fmt()
        .with_max_level(Level::INFO)
        .with_file(false)
        .init();

    info!("\x1b[93mAROWSS (Automatic Remote Onboard Wireless Streaming System)\x1b[0m \x1b[92minitalized.\x1b[0m");

    let rfd_port = serialport::new(RFD_PATH, RFD_BAUD)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .data_bits(serialport::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open()
        .unwrap();

    info!("RFD-900x serial port open on {RFD_PATH}");

    let rfd_send = rfd_port.try_clone().unwrap();
    let rfd_recv = rfd_port.try_clone().unwrap();

    let (info_send, info_recv) = mpsc::channel();

    // Spawn and wait on the tasks until they finish, which they should never
    let send = tokio::spawn(sending_loop(rfd_send, info_recv));
    let recv = tokio::spawn(command_loop(rfd_recv, info_send));

    info!("Waiting on tasks...");
    #[allow(unused_must_use)]
    {
        join!(send, recv);
    }
}

#[instrument(skip_all)]
async fn sending_loop(mut rfd_send: Box<dyn SerialPort>, info_recv: Receiver<String>) {
    info!("Initalized telemetry sending");

    let i2c = Arc::new(Mutex::new(I2cdev::new("/dev/i2c-1").unwrap()));

    // Spawn GPS task
    let (gps_send, gps_recv) = watch::channel(None);
    tokio::spawn(gps_loop(gps_send));
    info!("Spawned GPS task");

    // Spawn BMP task
    let (bmp_send, bmp_recv) = watch::channel((None,None));
    let bmpi2c = Arc::clone(&i2c);
    tokio::spawn(async move {
        let bmpi2c = MutexDevice::new(&*bmpi2c);
        bmp_loop(bmp_send, bmpi2c).await;
    });
    info!("Spawned BMP task");

    // Spawn BNO task
    let (bno_send, bno_recv) = watch::channel(None);
    let bnoi2c = Arc::clone(&i2c);
    tokio::spawn(async move {
        let bnoi2c = MutexDevice::new(&*bnoi2c);
        bno055_loop(bno_send, bnoi2c).await;
    });
    info!("Spawned BNO task");

    let mut info_deque = VecDeque::new();

    let mut sending_interval = time::interval(Duration::from_millis(250));
    sending_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    //
    // Every packet begins with a CRC as a decimal number, followed by a space
    // followed by the JSON data, and terminated by a newline (`\n`).
    loop {
        if let Ok(i) = info_recv.try_recv() {
            info_deque.push_back(i);

            if info_deque.len() >= 4 {
                info_deque.pop_front();
            }
        }

        // Construct a packet from the data
        let bmp_data = *bmp_recv.borrow();
        let pressure = bmp_data.0.unwrap_or(0.0);
        let temperature = bmp_data.1.unwrap_or(0.0);

        let env_info = EnvironmentalInfo {
            pressure,
            temperature,
            humidity: 0.0,
        };

        let packet = TelemetryPacket {
            gps: *gps_recv.borrow(),
            environmental_info: Some(env_info),
            info: info_deque.clone(),
        };

        // Calculate the CRC of the packet based on its data.
        let (packet_bytes, packet_crc) = packet.vec_crc();

        if packet_bytes.len() > MAX_PACKET_BYTES {
            warn!("Packet size of {} bytes exceeds max of {MAX_PACKET_BYTES}", packet_bytes.len());
        }

        // Write the data out
        rfd_send
            .write_all(packet_crc.to_string().as_bytes())
            .unwrap();
        rfd_send.write_all(b" ").unwrap();
        rfd_send.write_all(&packet_bytes).unwrap();
        rfd_send.write_all(b"\n").unwrap();

        debug!("Sent {} bytes, checksum {}", packet_bytes.len(), packet_crc);

        rfd_send.flush().unwrap();

        sending_interval.tick().await;
    }
}

const HIGH_POWER_RELAY_PIN_NUM: u8 = 26;    //TODO set to actual pin being used

#[instrument(skip_all)]
async fn command_loop(mut rfd_recv: Box<dyn SerialPort>, info_send: Sender<String>) {
    info!("Initalized command receiving");

    // Set up relay GPIO pin
    let gpio = Gpio::new().expect("Unable to initalize GPIO pins");
    let mut relay_pin = gpio.get(HIGH_POWER_RELAY_PIN_NUM)
        .unwrap()
        .into_output();
    relay_pin.set_reset_on_drop(false);

    // Create command parser with devices
    let mut command_parser = CommandParser {
        relay_pin,
        info_sender: info_send,
    };

    // Each buffer must consist of 3 bytes:
    //  1. Command
    //  2. Checksum
    //  3. Space b' '
    //
    //  If the buffer violates this at any time, it must be discarded as
    //  invalid.
    let mut buf = Vec::new();
    loop {
        let mut byte_buf = [0];
        if rfd_recv.read_exact(&mut byte_buf).is_err() {
            continue;
        }

        buf.push(byte_buf[0]);

        if buf.len() > 3 || (buf.last() != Some(&b' ') && buf.len() == 3) {
            warn!("Buffer invalid: {:?}", buf);
            buf.clear();
            continue;
        }

        if buf.first() == Some(&b' ') || buf.get(1) == Some(&b' ') {
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

            match command_parser.parse_command(data).await {
                Ok(()) => (),
                Err(e) => error!("ERR: {e:?}, {e}"),
            }

            buf.clear();
        }
    }
}

/// Function to read the Ublox ZED-F9P GPS module.
#[instrument(skip_all)]
async fn gps_loop(data: watch::Sender<Option<GpsInfo>>) {
    // Set up the GPS serial port. This must utilize the proper port on the
    // raspberry pi.
    let mut gps_port = serialport::new(GPS_PATH, GPS_BAUD)
        .timeout(Duration::from_millis(50))
        .open()
        .unwrap();

    // Set up and configure the NMEA parser.
    let mut nmea_parser = Nmea::create_for_navigation(&[
        SentenceType::GGA, SentenceType::GLL, SentenceType::GNS,
        SentenceType::VTG, SentenceType::RMC
    ]).unwrap();

    let mut buffer = Vec::new();
    let mut byte_buf = [0u8; 1];

    loop {
        let bytes_read = gps_port.read(&mut byte_buf).unwrap_or_default();

        if bytes_read == 0 {
            continue;
        }

        // NMEA messages must end with '\r\n'
        if byte_buf[0] != b'\n' {
            buffer.push(byte_buf[0]);
            continue;
        }

        // NMEA messages must start with '$'
        if buffer[0] != b'$' {
            buffer.clear();
            continue;
        }

        // Create a String from the buffer and clear the buffer
        let new_string = String::from_utf8_lossy(&buffer).into_owned();
        let new_string = new_string.trim_end();
        buffer.clear();

        // info!("Got NMEA: {}", new_string);

        #[allow(clippy::single_match)]
        match nmea_parser.parse_for_fix(new_string) {
            Ok(_) => (),
            Err(_) => (),
        }

        if let Some(lat) = nmea_parser.latitude
            && let Some(lon) = nmea_parser.longitude
            && let Some(alt) = nmea_parser.altitude
        {
            let _ = data.send(Some(GpsInfo {
                latitude: lat,
                longitude: lon,
                altitude: alt,
                satellites: nmea_parser.satellites().len() as u8
            }));
        }
    }
}

/// Function to read the BMP581 pressure and temp sensor.
#[instrument(skip_all)]
async fn bmp_loop(data: watch::Sender<(Option<f64>, Option<f64>)>, i2c: MutexDevice<'_, I2cdev>) {
    let mut bmp = Bmp581::new_i2c(i2c, I2cAddr::Default);
    let mut delay = linux_embedded_hal::Delay;

    if bmp.init(&mut delay).is_err() {
        error!("Could not initalize BMP581");
        return
    };

    // Set up measurement settings
    bmp.set_osr_config(bmp581::types::OsrConfig {
        pressure_enable: true,
        osr_pressure: Osr::Osr8,
        osr_temperature: Osr::Osr1,
    }).unwrap();

    // Set up output rate settings
    bmp.set_odr_config(bmp581::types::OdrConfig {
        deep_dis: DeepDis::Disabled,
        odr: Odr::Hz20_000,
        power_mode: PowerMode::Normal,
    }).unwrap();

    loop {
        sleep(Duration::from_millis(50)).await;

        if let Ok(temp) = bmp.read_temperature() && let Ok(pres) = bmp.read_pressure() {
            let _ = data.send((Some(pres as f64), Some(temp as f64)));
        }
    }
}

#[instrument(skip_all)]
async fn bno055_loop(data: watch::Sender<Option<(f64, f64)>>, i2c: MutexDevice<'_, I2cdev>) {
    let mut bno055 = bno055::Bno055::new(i2c);
    let mut delay = linux_embedded_hal::Delay;
    bno055.set_mode(bno055::BNO055OperationMode::NDOF, &mut delay).unwrap();
    bno055.set_power_mode(BNO055PowerMode::NORMAL).unwrap();
    bno055.init(&mut delay).unwrap();
}

// #[instrument(skip_all)]
// async fn hts221_loop(data: watch::Sender<Option<(f64, f64)>>, mut i2c: MutexDevice<'_, I2cdev>) {
//     let mut delay = linux_embedded_hal::Delay;
//     let mut hts = hts221::Builder::new().with_data_rate(hts221::DataRate::Continuous1Hz).build(&mut i2c).unwrap();
// }
