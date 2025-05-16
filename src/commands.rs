use num_derive::{FromPrimitive, ToPrimitive};
use rppal::gpio::Gpio;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tracing::warn;

/// Commands which the air side code must respond to from the ground.
#[derive(FromPrimitive, ToPrimitive, Debug, Clone, Copy)]
#[non_exhaustive]
pub enum UplinkCommand {
    /// Enable the High Power components via the relay
    EnableHighPower = 71,
    /// Disable the High Power components via the relay
    DisableHighPower = 72,

    /// Start recording on the Runcams
    EnableRuncams = 80,
    /// Start recording on the Runcams
    DisableRuncams = 81,
}

const HIGH_POWER_RELAY_PIN_NUM: u8 = 26;    //TODO set to actual pin being used

pub async fn command_loop(command_rx: mpsc::Receiver<UplinkCommand>) {
    // Set up relay control pin
    let gpio = Gpio::new().expect("Unable to initalize GPIO pins");
    let mut relay_pin = gpio.get(HIGH_POWER_RELAY_PIN_NUM)
        .unwrap()
        .into_output_low();

    let mut command_stream = ReceiverStream::new(command_rx);

    while let Some(command) = command_stream.next().await {
        match command {
            UplinkCommand::EnableHighPower => relay_pin.set_high(),
            UplinkCommand::DisableHighPower => relay_pin.set_low(),
            invalid_cmd => warn!("Command {invalid_cmd:?} not implemented"),
        }
    }
}
