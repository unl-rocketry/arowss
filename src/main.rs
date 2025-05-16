mod commands;
use byteorder_lite::{ReadBytesExt, WriteBytesExt};
use commands::{command_loop, UplinkCommand};

use arowss::{utils::crc8, EnvironmentalInfo, GpsInfo, PowerInfo, TelemetryPacket};
use bmp388::{BMP388, PowerControl};
use ina219::SyncIna219;
use linux_embedded_hal::I2cdev;
use num_traits::FromPrimitive;
use tracing::{warn, debug, error, info, instrument, Level};
use nmea::{Nmea, SentenceType};
use std::time::Duration;
use tokio::{
    join, sync::{mpsc, watch},
    time::{self, sleep},
};
use serialport::SerialPort;

const RFD_PATH: &str = "/dev/ttyAMA2";
const RFD_BAUD: u32 = 57600;
/// This is the maximum number of bytes that can be sent by the RFD-900 per
/// packet without dropping behind
const MAX_PACKET_BYTES: usize = (RFD_BAUD as usize / 9) / 4;


const GPS_PATH: &str = "/dev/ttyAMA3";
const GPS_BAUD: u32 = 38400;


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

    // Spawn and wait on the tasks until they finish, which they should never
    let send = tokio::spawn(sending_loop(rfd_send));

    // Set up command channel and run task for command actions
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(100);
    let command_loop = tokio::spawn(command_loop(command_rx));
    let command_receiver = tokio::spawn(command_receiver(rfd_recv, command_tx));

    info!("Waiting on tasks...");
    #[allow(unused_must_use)]
    {
        join!(send, command_receiver, command_loop);
    }
}

#[instrument(skip_all)]
async fn sending_loop(mut rfd_send: Box<dyn SerialPort>) {
    info!("Initalized telemetry sending");

    // Spawn GPS task
    let (gps_send, gps_recv) = watch::channel(None);
    tokio::spawn(gps_loop(gps_send));
    info!("Spawned GPS task");

    // Spawn INA task
    let (ina_send, ina_recv) = watch::channel(None);
    tokio::spawn(ina_loop(ina_send));
    info!("Spawned INA task");

    // Spawn BMP task
    let (bmp_send, bmp_recv) = watch::channel(None);
    tokio::spawn(bmp_loop(bmp_send));
    info!("Spawned BMP task");

    let mut sending_interval = time::interval(Duration::from_millis(250));
    sending_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    let mut sequence_number = 0;

    // Main packet sending loop. A packet should be sent 4 times per second,
    // every 250ms. The packet format should allow for individual parts of
    // the packet information to be unavailable so any single part failing
    // cannot take down the whole system.
    //
    // Every packet begins with a CRC as a decimal number, followed by a space
    // followed by the JSON data, and terminated by a newline (`\n`).
    loop {
        // Construct a packet from the data
        let packet = TelemetryPacket::builder()
            .sequence_number(sequence_number)
            .maybe_gps(*gps_recv.borrow())
            .maybe_power_info(*ina_recv.borrow())
            .maybe_environmental_info(*bmp_recv.borrow())
            .build();

        // Increment the sequence number
        sequence_number = sequence_number.wrapping_add(1);

        // Calculate the CRC of the packet based on its data.
        let (packet_bytes, packet_crc) = packet.vec_crc();

        if packet_bytes.len() > MAX_PACKET_BYTES {
            warn!("Packet size of {} bytes exceeds max of {MAX_PACKET_BYTES}", packet_bytes.len());
        }

        // Write the data out
        rfd_send.write_u8(packet_crc).unwrap();
        rfd_send.write_u8(b' ').unwrap();
        rfd_send.write_all(&packet_bytes).unwrap();
        rfd_send.write_u8(b'\n').unwrap();

        debug!("Sent {} bytes, checksum {}", packet_bytes.len(), packet_crc);

        rfd_send.flush().unwrap();

        sending_interval.tick().await;
    }
}

#[instrument(skip_all)]
async fn command_receiver(mut rfd_recv: Box<dyn SerialPort>, command_tx: mpsc::Sender<UplinkCommand>) {
    info!("Initalized command receiving");

    // Each buffer must consist of 3 bytes:
    //  1. Command
    //  2. Checksum
    //  3. Space b' '
    //
    //  If the buffer violates this at any time, it must be discarded as
    //  invalid.
    let mut buf = Vec::new();
    loop {
        let Ok(recv_byte) = rfd_recv.read_u8() else {
            continue;
        };

        buf.push(recv_byte);

        if buf.len() > 3 || (buf.len() == 3 && buf.last() != Some(&b' ')) {
            warn!("Buffer invalid: {:?}", buf);
            buf.clear();
            continue;
        } else if buf.len() < 3 && buf.contains(&b' ') {
            warn!("Buffer invalid: {:?}", buf);
            buf.clear();
            continue;
        } else if buf.len() != 3 {
            // Can only parse properly if there are 3 bytes in the buffer
            continue;
        }

        info!("Got command buffer {:?}", buf);

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


        match UplinkCommand::from_u8(data) {
            Some(c) => if let Err(e) = command_tx.send(c).await {
                println!("Could not send command: {e}");
            },
            None => warn!("Got invalid command {data}"),
        }

        // Clear the buffer to get the next message
        buf.clear();
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

        match nmea_parser.parse_for_fix(new_string) {
            Ok(_) => (),
            Err(e) => warn!("{e:?}"),
        }

        let _ = data.send(Some(GpsInfo {
            datetime: nmea_parser.fix_timestamp().unwrap_or_default(),
            latitude: nmea_parser.latitude(),
            longitude: nmea_parser.longitude(),
            altitude: nmea_parser.altitude(),
        }));
    }
}

/// Function to read the INA219 current sensor.
#[instrument(skip_all)]
async fn ina_loop(data: watch::Sender<Option<PowerInfo>>) {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let Ok(mut ina) = SyncIna219::new(i2c, ina219::address::Address::from_byte(0x40).unwrap()) else {
        error!("Could not initalize INA219");
        return
    };

    loop {
        sleep(Duration::from_millis(250)).await;

        let _ = data.send(Some(PowerInfo {
            voltage: ina.bus_voltage().unwrap_or_default().voltage_mv(),
            current: ina.current_raw().unwrap_or_default().0,
        }));
    }
}

/// Function to read the BMP388 pressure and temp sensor.
#[instrument(skip_all)]
async fn bmp_loop(data: watch::Sender<Option<EnvironmentalInfo>>) {
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

        let _ = data.send(Some(EnvironmentalInfo {
            pressure: sensor_data.pressure,
            temperature: sensor_data.temperature,
        }));
    }
}
