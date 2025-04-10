use std::time::Duration;
use tokio_serial::SerialPortBuilderExt;

struct RunCam {
    port: tokio_serial::SerialStream
}

impl RunCam {
    pub fn new(port: &str) -> Self {
        // Open port for runcam
        let mut runcam_port = tokio_serial::new(port, 57600)
            .timeout(Duration::from_millis(50))
            .open_native_async()
            .unwrap();
        Self {
            port: runcam_port
        }
    }
}

enum ActionSimulate {
    WifiButton = 0x0,
    PowerButton = 0x1,
    ChangeMode = 0x2,
    StartRecording = 0x3,
    StopRecording = 0x4
}

// Packet to send to the runcam. Contains a header, command ID, the 
// action ID of the desired runcam action, and the crc for the packet.
struct RequestPacket {
    header: u8,
    command_id: u8,
    action_id: u8,
    crc: u8
}

impl Default for RequestPacket {
    fn default() -> Self {
        Self {
            header: 0xCC, 
            command_id: 0x01, 
            action_id: Default::default(), 
            crc: Default::default() 
        }
    }
}

impl RequestPacket {
    pub fn insert_crc(&mut self) {
        // Create input array for crc8 calculation
        let crc_array = [
            self.header,
            self.command_id,
            self.action_id 
        ];
        self.crc = crc8(&crc_array)
    } 
}

// Calculate the crc for the packet
fn crc8(arr: &[u8]) -> u8 {
    let mut crc = 0x00;
    for element in arr {
        crc ^= element; 
        for _i in 1..8 {
            if crc & 0x80 > 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}