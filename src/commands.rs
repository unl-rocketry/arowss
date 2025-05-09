use arowss::runcam::{self, RunCam};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use rppal::gpio::OutputPin;

/// Commands which the air side code must respond to from the ground.
#[derive(FromPrimitive, ToPrimitive)]
#[repr(u8)]
pub enum Commands {
    /// Enable the High Power components via the relay
    EnableHighPower = 70,
    /// Disable the High Power components via the relay
    DisableHighPower = 80,

    /// Start recording on the Runcams
    EnableRuncams = 90,

    /// Start recording on the Runcams
    DisableRuncams = 100,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseErr {
    #[error("Command is not valid")]
    Invalid,
}

// Struct containing items which need to be modified by ground commands.
pub struct CommandParser {
    pub relay_pin: OutputPin,
    pub runcam: Option<RunCam>,
}

impl CommandParser {
    pub fn parse_command(&mut self, data: u8) -> Result<(), ParseErr> {
        let Some(command) = Commands::from_u8(data) else {
            return Err(ParseErr::Invalid)
        };

        match command {
            Commands::EnableHighPower => self.relay_pin.set_high(),
            Commands::DisableHighPower => self.relay_pin.set_low(),
            Commands::EnableRuncams => if let Some(r) = self.runcam.as_mut() {
                let _ = r.write_camera_control(runcam::ControlActions::StartRecording);
            },
            Commands::DisableRuncams => if let Some(r) = self.runcam.as_mut() {
                let _ = r.write_camera_control(runcam::ControlActions::StopRecording);
            },
        }

        Ok(())
    }
}
