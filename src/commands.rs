use std::{fs, io::Write, sync::mpsc::Sender};

use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use rppal::gpio::OutputPin;

/// Commands which the air side code must respond to from the ground.
#[derive(FromPrimitive, ToPrimitive)]
#[repr(u8)]
#[non_exhaustive]
pub enum Commands {
    /// Enable the Taisync radio
    EnableHighPower = 70,
    /// Disable the Taisync radio
    DisableHighPower = 80,

    /// Forcibly reboot without waiting for any processes to finish
    Reboot = 100,
    /// Restart the stream process
    RestartStream = 101,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseErr {
    #[error("Command is not valid")]
    Invalid,
}

// Struct containing items which need to be modified by ground commands.
pub struct CommandParser {
    pub relay_pin: OutputPin,
    pub info_sender: Sender<String>,
}

impl CommandParser {
    pub async fn parse_command(&mut self, data: u8) -> Result<(), ParseErr> {
        let Some(command) = Commands::from_u8(data) else {
            return Err(ParseErr::Invalid)
        };

        match command {
            Commands::EnableHighPower => {
                self.relay_pin.set_high();
                self.info_sender.send("Relay enabled".to_string());
            }
            Commands::DisableHighPower => {
                self.relay_pin.set_low();
                self.info_sender.send("Relay disabled".to_string());
            }
            Commands::Reboot => {
                if let Ok(mut reboot_file) = fs::File::create("/proc/sysrq-trigger") {
                    let _ = reboot_file.write_all(b"b");
                }
            }
            Commands::RestartStream => {
                let _ = std::process::Command::new("systemctl")
                    .arg("restart")
                    .arg("streaming.service")
                    .spawn();
            }
            //_ => warn!("Invalid command"),
        }

        Ok(())
    }
}
