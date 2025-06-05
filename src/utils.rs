use serde::Serializer;

/// Calculate the CRC for some arbitrary data.
#[must_use]
pub fn crc8(arr: &[u8]) -> u8 {
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

/// Calculate the NMEA CRC for some arbitrary data.
#[must_use]
pub fn nmea_crc8(arr: &[u8]) -> u8 {
    let mut crc = 0x00;

    for element in arr {
        crc ^= element;
    }

    crc
}

/// Create an NMEA sentence to send to a GPS to control it.
#[must_use]
pub fn create_nmea_command(cmd: &str) -> Vec<u8> {
    format!(
        "${cmd}*{:02X}\r\n",
        nmea_crc8(cmd.as_bytes())
    ).as_bytes().to_vec()
}

pub fn truncate_float<S>(float: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
{
    serializer.serialize_str(&format!("{float:.3}"))
}
