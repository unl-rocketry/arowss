use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;

pub struct RunCam {
    port: tokio_serial::SerialStream,
}

impl RunCam {
    pub fn new(port: &str) -> Self {
        // Open port for runcam
        let runcam_port = tokio_serial::new(port, 115200)
            .timeout(Duration::from_millis(50))
            .open_native_async()
            .unwrap();

        Self { port: runcam_port }
    }

    pub async fn get_camera_information(&mut self) -> (u8, u16) {
        let data = [0xCC, CommandIds::ReadCameraInformation as u8];
        let crc = crc8(&data);

        self.port.write_all(&data).await.unwrap();
        self.port.write_u8(crc).await.unwrap();

        let _ = self.port.read_u8().await.unwrap();
        let protocol_version = self.port.read_u8().await.unwrap();
        let feature = self.port.read_u16().await.unwrap();
        let _ret_crc = self.port.read_u8().await.unwrap();

        (protocol_version, feature)
    }

    pub async fn write_camera_control(&mut self, action: ControlActions) {
        let data = [0xCC, CommandIds::CameraControl as u8, action as u8];
        let crc = crc8(&data);

        self.port.write_all(&data).await.unwrap();
        self.port.write_u8(crc).await.unwrap();
    }
}

#[derive(Clone, Copy)]
pub enum CommandIds {
    ReadCameraInformation = 0x00,
    CameraControl = 0x01,
    SimulatePress = 0x02,
    SimulateRelease = 0x03,
    SimulateHandshake = 0x04,
}

#[derive(Clone, Copy)]
pub enum ControlActions {
    WifiButton = 0x00,
    PowerButton = 0x01,
    ChangeMode = 0x02,
    StartRecording = 0x03,
    StopRecording = 0x04,
}

/// Calculate the crc for the packet
fn crc8(arr: &[u8]) -> u8 {
    let mut crc = 0x00;
    for element in arr {
        crc ^= element;
        for _ in 0..8 {
            if crc & 0x80 > 0 {
                crc = (crc << 1) ^ 0xd5;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
