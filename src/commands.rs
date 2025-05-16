use std::collections::VecDeque;

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
const COMMAND_LIST_LEN: usize = 16;

pub async fn command_loop(command_rx: mpsc::Receiver<UplinkCommand>) {
    let gpio = Gpio::new().expect("Unable to initalize GPIO pins");

    // Set up relay control pin
    let mut relay_pin = gpio.get(HIGH_POWER_RELAY_PIN_NUM)
        .unwrap()
        .into_output_low();
    relay_pin.set_reset_on_drop(false);

    let mut command_stream = ReceiverStream::new(command_rx);

    // A list of the COMMAND_LIST_LEN latest commands received
    let mut command_list = VecDeque::new();

    while let Some(command) = command_stream.next().await {
        match command {
            UplinkCommand::EnableHighPower => relay_pin.set_high(),
            UplinkCommand::DisableHighPower => relay_pin.set_low(),
            invalid_cmd => warn!("Command {invalid_cmd:?} not implemented"),
        }

        command_list.push_back(command);

        if command_list.len() > COMMAND_LIST_LEN {
            command_list.pop_front();
        }
    }
}
