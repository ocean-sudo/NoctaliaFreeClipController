//! Huawei SPP framing.
//!
//! A packet is `5A | u16be length | 00 | body | crc16-xmodem`.
//! `length` counts the `00` byte plus the body. The body starts with a
//! big-endian command id, then type/length/value parameters.

/// CRC-16/XMODEM: polynomial 0x1021, initial value 0.
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub command: u16,
    pub params: Vec<(u8, Vec<u8>)>,
}

impl Packet {
    pub fn new(command: u16, params: Vec<(u8, Vec<u8>)>) -> Self {
        Self { command, params }
    }

    pub fn read(command: u16, ids: &[u8]) -> Self {
        Self {
            command,
            params: ids.iter().map(|id| (*id, Vec::new())).collect(),
        }
    }

    pub fn with_i8(command: u16, params: &[(u8, i8)]) -> Self {
        Self {
            command,
            params: params
                .iter()
                .map(|(id, value)| (*id, vec![*value as u8]))
                .collect(),
        }
    }

    pub fn param(&self, id: u8) -> Option<&[u8]> {
        self.params
            .iter()
            .find(|(kind, _)| *kind == id)
            .map(|(_, value)| value.as_slice())
    }

    pub fn param_i8(&self, id: u8) -> Option<i8> {
        self.param(id)
            .filter(|value| value.len() == 1)
            .map(|value| value[0] as i8)
    }

    pub fn param_u8(&self, id: u8) -> Option<u8> {
        self.param(id)
            .filter(|value| value.len() == 1)
            .map(|value| value[0])
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(8 + self.params.len() * 4);
        body.extend_from_slice(&self.command.to_be_bytes());
        for (kind, value) in &self.params {
            body.push(*kind);
            body.push(u8::try_from(value.len()).expect("SPP parameter longer than 255 bytes"));
            body.extend_from_slice(value);
        }

        let length = u16::try_from(body.len() + 1).expect("SPP packet longer than 65535 bytes");
        let mut packet = Vec::with_capacity(body.len() + 6);
        packet.push(0x5A);
        packet.extend_from_slice(&length.to_be_bytes());
        packet.push(0x00);
        packet.extend_from_slice(&body);
        packet.extend_from_slice(&crc16_xmodem(&packet).to_be_bytes());
        packet
    }

    pub fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 6 || data[0] != 0x5A {
            return Err("not an SPP packet".into());
        }
        let length = usize::from(u16::from_be_bytes([data[1], data[2]]));
        let expected = length + 5;
        if data.len() != expected {
            return Err(format!(
                "SPP length is {length}, packet is {} bytes",
                data.len()
            ));
        }
        if data[3] != 0x00 {
            return Err("SPP header flag is not 0x00".into());
        }
        let crc_at = data.len() - 2;
        let got = u16::from_be_bytes([data[crc_at], data[crc_at + 1]]);
        let calc = crc16_xmodem(&data[..crc_at]);
        if got != calc {
            return Err(format!("SPP checksum {got:04x} does not match {calc:04x}"));
        }

        let body = &data[4..crc_at];
        if body.len() < 2 {
            return Err("SPP body is missing the command".into());
        }
        let command = u16::from_be_bytes([body[0], body[1]]);
        let mut params = Vec::new();
        let mut index = 2;
        while index < body.len() {
            if index + 1 >= body.len() {
                return Err("truncated SPP parameter".into());
            }
            let kind = body[index];
            let size = usize::from(body[index + 1]);
            let start = index + 2;
            let end = start + size;
            if end > body.len() {
                return Err("SPP parameter extends past the body".into());
            }
            params.push((kind, body[start..end].to_vec()));
            index = end;
        }
        Ok(Self { command, params })
    }
}

pub async fn read_packet<R>(reader: &mut R) -> Result<Packet, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    loop {
        let mut magic = [0u8; 1];
        reader
            .read_exact(&mut magic)
            .await
            .map_err(|err| err.to_string())?;
        if magic[0] == 0x5A {
            break;
        }
    }

    let mut len_bytes = [0u8; 2];
    reader
        .read_exact(&mut len_bytes)
        .await
        .map_err(|err| err.to_string())?;
    let length = usize::from(u16::from_be_bytes(len_bytes));
    if length > 1024 {
        return Err(format!("SPP length {length} is not plausible"));
    }
    let mut rest = vec![0u8; length + 2];
    reader
        .read_exact(&mut rest)
        .await
        .map_err(|err| err.to_string())?;

    let mut packet = Vec::with_capacity(3 + rest.len());
    packet.push(0x5A);
    packet.extend_from_slice(&len_bytes);
    packet.extend_from_slice(&rest);
    Packet::decode(&packet)
}

/// Packet logging is on unless `FREECLIP_DEBUG` is `0`, `off`, or `false`.
pub fn packet_log_enabled() -> bool {
    !matches!(
        std::env::var("FREECLIP_DEBUG").ok().as_deref(),
        Some("0") | Some("off") | Some("false")
    )
}

pub fn packet_log_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("noctalia-freeclip").join("packets.log")
}

pub fn format_packet_line(direction: &str, packet: &Packet) -> String {
    let mut params = String::new();
    for (id, value) in &packet.params {
        if !params.is_empty() {
            params.push(' ');
        }
        params.push_str(&format!("{id:02x}:{}", hex_encode(value)));
    }
    format!(
        "{} {direction} {:04x} {params} {}",
        local_stamp(),
        packet.command,
        hex_encode(&packet.encode())
    )
}

pub fn log_packet(direction: &str, packet: &Packet) {
    if !packet_log_enabled() {
        return;
    }
    let line = format_packet_line(direction, packet);
    if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        eprintln!("{line}");
    }
    append_packet_log(&line);
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn local_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let millis = now.subsec_millis();
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let ok = unsafe { libc::localtime_r(&secs, &mut tm) };
    if ok.is_null() {
        return format!("{secs}.{millis:03}");
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        millis
    )
}

fn append_packet_log(line: &str) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut state = PACKET_LOG.lock().unwrap_or_else(|err| err.into_inner());
    let path = packet_log_path();
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            if !state.failed {
                eprintln!("freeclipd: packet log {}: {err}", path.display());
                state.failed = true;
            }
            return;
        }
    }
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 1024 * 1024 {
            let rotated = path.with_extension("log.1");
            let _ = std::fs::rename(&path, &rotated);
        }
    }
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(err) => {
            if !state.failed {
                eprintln!("freeclipd: packet log {}: {err}", path.display());
                state.failed = true;
            }
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        if !state.failed {
            eprintln!("freeclipd: packet log {}: {err}", path.display());
            state.failed = true;
        }
    }
}

struct PacketLogState {
    failed: bool,
}

static PACKET_LOG: std::sync::Mutex<PacketLogState> =
    std::sync::Mutex::new(PacketLogState { failed: false });

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(value: &str) -> Vec<u8> {
        (0..value.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn packet_log_line_contains_the_frame() {
        let packet = Packet::read(0x0108, &[1, 2, 3]);
        let line = format_packet_line("tx", &packet);
        assert!(line.contains(" tx 0108 "), "{line}");
        assert!(line.contains("01: 02: 03:"), "{line}");
        assert!(line.contains("5a0009000108010002000300fbb9"), "{line}");
    }

    #[test]
    fn battery_read_matches_captured_frame() {
        let packet = Packet::read(0x0108, &[1, 2, 3]);
        assert_eq!(packet.encode(), hex("5a0009000108010002000300fbb9"));
    }

    #[test]
    fn auto_pause_frames_match_captured_traffic() {
        assert_eq!(
            Packet::read(0x2b11, &[1]).encode(),
            hex("5a0005002b110100772a")
        );
        assert_eq!(
            Packet::with_i8(0x2b10, &[(1, 1)]).encode(),
            hex("5a0006002b10010101a956")
        );
    }

    #[test]
    fn gesture_writes_match_captured_traffic() {
        assert_eq!(
            Packet::read(0x0120, &[1, 2]).encode(),
            hex("5a000700012001000200e897")
        );
        assert_eq!(
            Packet::with_i8(0x011f, &[(1, 2)]).encode(),
            hex("5a000600011f01010203c1")
        );
        assert_eq!(
            Packet::with_i8(0x2b1e, &[(1, -1), (2, -1)]).encode(),
            hex("5a0009002b1e0101ff0201ff9d9b")
        );
        assert_eq!(
            Packet::read(0x2b1f, &[1, 2]).encode(),
            hex("5a0007002b1f01000200328a")
        );
    }

    #[test]
    fn decode_round_trips_and_exposes_signed_values() {
        let packet = Packet::decode(&hex("5a0006002b11010100cfc3")).unwrap();
        assert_eq!(packet.command, 0x2b11);
        assert_eq!(packet.param_i8(1), Some(0));

        let battery =
            Packet::decode(&hex("5a0014000108010140020310203003030001000402140a1461")).unwrap();
        assert_eq!(battery.param_u8(1), Some(0x40));
        assert_eq!(battery.param(2), Some(&[0x10, 0x20, 0x30][..]));
        assert_eq!(battery.param(3), Some(&[0x00, 0x01, 0x00][..]));

        let taps = Packet::decode(&hex(
            "5a0017000120010101020102030501070200ff0401ff060200ffcf60",
        ))
        .unwrap();
        assert_eq!(taps.param_i8(1), Some(1));
        assert_eq!(taps.param_i8(2), Some(2));
        assert_eq!(taps.param(3), Some(&[1, 7, 2, 0, 0xff][..]));
    }
}
