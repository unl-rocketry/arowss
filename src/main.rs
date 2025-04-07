use ina219::{address::Address, SyncIna219};
use linux_embedded_hal::I2cdev;

fn main() {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();
    let mut ina = SyncIna219::new(i2c, Address::from_byte(0x40).unwrap()).unwrap();

    loop {
        std::thread::sleep(ina.configuration().unwrap().conversion_time().unwrap());

        println!("Bus Voltage: {}", ina.bus_voltage().unwrap());
        println!("Shunt Voltage: {}", ina.shunt_voltage().unwrap());
        println!("Current: {:?}", ina.current_raw().unwrap());
        println!("Power: {:?}", ina.power_raw().unwrap());
    }
}
