use arowss::{utils::crc8, EnvironmentalInfo, GpsInfo, PowerInfo, TelemetryPacket};
use bmp388::{BMP388, PowerControl};
use ina219::SyncIna219;
use linux_embedded_hal::I2cdev;
use nmea::{Nmea, SentenceType};
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _}, join, sync::Mutex, time::{sleep, sleep_until, Instant}
};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    let send = tokio::spawn(sending_loop());
    let recv = tokio::spawn(command_loop());

    #[allow(unused_must_use)]
    {
        join!(send, recv);
    }
}

async fn sending_loop() {
    let mut rfd_send = tokio_serial::new("/dev/ttyUSB0", 57600)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .data_bits(tokio_serial::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    rfd_send.set_exclusive(false).unwrap();

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
        rfd_send.write_all(packet_crc.to_string().as_bytes()).await.unwrap();
        rfd_send.write_u8(b' ').await.unwrap();
        let json_vec = serde_json::to_vec(&packet).unwrap();
        rfd_send.write_all(&json_vec).await.unwrap();
        rfd_send.write_u8(b'\n').await.unwrap();

        // If there is any time left over, sleep
        sleep_until(timeout).await;
    }
}

async fn command_loop() {
    let mut rfd_recv = tokio_serial::new("/dev/ttyUSB0", 57600)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .data_bits(tokio_serial::DataBits::Eight)
        .timeout(Duration::from_millis(50))
        .open_native_async()
        .unwrap();

    rfd_recv.set_exclusive(false).unwrap();

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
                println!("Checksums do not match!");
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

        for line in new_string.lines()
            .filter(|l| !l.is_empty())
            .filter(|l| l.starts_with("$"))
        {
            let _ = nmea_parser.parse_for_fix(line);
        }

        if nmea_parser.latitude.is_none() || nmea_parser.longitude.is_none() || nmea_parser.altitude.is_none() {
            continue
        }

        *data.lock().await = Some(GpsInfo {
            latitude: nmea_parser.latitude.unwrap(),
            longitude: nmea_parser.longitude.unwrap(),
            altitude: nmea_parser.altitude.unwrap(),
        });
    }
}

/// Function to read the INA219 current sensor.
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

/// Function to read the BMP388 pressure and temp sensor.
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


pub enum Commands {
    ExampleCommand1,
    ExampleCommand2,
    ExampleCommand3,
    ExampleCommand4,
}
impl Commands {
    pub fn from_string(input: u8) -> Self {
        match input {
            1 => Commands::ExampleCommand1,
            2 => Commands::ExampleCommand2,
            3 => Commands::ExampleCommand3,
            4 => Commands::ExampleCommand4,
            _ => panic!(),
        }
    }
}
