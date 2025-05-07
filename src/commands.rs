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
}

#[derive(Debug, thiserror::Error)]
pub enum ParseErr {
    #[error("Command is not valid")]
    Invalid,
}

pub async fn parse_command(data: u8, relay_pin: &mut OutputPin) -> Result<(), ParseErr> {
    let Some(command) = Commands::from_u8(data) else {
        return Err(ParseErr::Invalid)
    };

    match command {
        Commands::EnableHighPower => {
            relay_pin.set_high();
        }
        Commands::DisableHighPower => {
            relay_pin.set_low();
        }
    }

    Ok(())
}
