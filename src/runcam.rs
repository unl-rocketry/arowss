use std::{error::Error, io, time::Duration};
use byteorder_lite::{ReadBytesExt as _, WriteBytesExt as _, LE};
use serialport::SerialPort;

use crate::utils::crc8;

pub struct RunCam {
    port: Box<dyn SerialPort>,
}

impl RunCam {
    pub fn new(port: &str) -> Result<Self, Box<dyn Error>> {
        // Open port for runcam
        let runcam_port = serialport::new(port, 115_200)
            .timeout(Duration::from_millis(50))
            .open()?;

        Ok(Self { port: runcam_port })
    }

    pub async fn get_camera_information(&mut self) -> Result<(u8, u16), io::Error> {
        let data = [0xCC, CommandIds::ReadCameraInformation as u8];
        let crc = crc8(&data);

        self.port.write_all(&data)?;
        self.port.write_u8(crc)?;

        let _ = self.port.read_u8()?;
        let protocol_version = self.port.read_u8()?;
        let feature = self.port.read_u16::<LE>()?;
        let _ret_crc = self.port.read_u8()?;

        Ok((protocol_version, feature))
    }

    pub async fn write_camera_control(&mut self, action: ControlActions) -> Result<(), io::Error> {
        let data = [0xCC, CommandIds::CameraControl as u8, action as u8];
        let crc = crc8(&data);

        self.port.write_all(&data)?;
        self.port.write_u8(crc)?;

        Ok(())
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
