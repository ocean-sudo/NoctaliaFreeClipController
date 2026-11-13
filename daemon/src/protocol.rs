//! FreeClip 2 feature commands.
//!
//! Command numbers follow the Huawei SPP profile used by FreeClip 2
//! (model BTFT0027, RFCOMM channel 1): battery, wear notice, gestures,
//! equalizer presets, sound-quality preference, low latency, auto-pause,
//! voice language, and dual-device records.

use serde::Serialize;
use serde_json::{json, Value};

use crate::spp::Packet;

pub const CMD_INFO: u16 = 0x0107;
pub const CMD_BATTERY: u16 = 0x0108;
pub const CMD_BATTERY_NOTIFY: u16 = 0x0127;
pub const CMD_DOUBLE_WRITE: u16 = 0x011f;
pub const CMD_DOUBLE_READ: u16 = 0x0120;
pub const CMD_TRIPLE_WRITE: u16 = 0x0125;
pub const CMD_TRIPLE_READ: u16 = 0x0126;
pub const CMD_LANGUAGE_WRITE: u16 = 0x0c01;
pub const CMD_LANGUAGE_READ: u16 = 0x0c02;
pub const CMD_WEARING: u16 = 0x2b03;
pub const CMD_AUTO_PAUSE_WRITE: u16 = 0x2b10;
pub const CMD_AUTO_PAUSE_READ: u16 = 0x2b11;
pub const CMD_LONG_WRITE: u16 = 0x2b16;
pub const CMD_LONG_READ: u16 = 0x2b17;
pub const CMD_NOISE_WRITE: u16 = 0x2b18;
pub const CMD_NOISE_READ: u16 = 0x2b19;
pub const CMD_SWIPE_WRITE: u16 = 0x2b1e;
pub const CMD_SWIPE_READ: u16 = 0x2b1f;
pub const CMD_DUAL_ENABLED_WRITE: u16 = 0x2b2e;
pub const CMD_DUAL_ENABLED_READ: u16 = 0x2b2f;
pub const CMD_DUAL_ENUMERATE: u16 = 0x2b31;
pub const CMD_DUAL_PREFER: u16 = 0x2b32;
pub const CMD_DUAL_EXECUTE: u16 = 0x2b33;
pub const CMD_DUAL_CHANGED: u16 = 0x2b36;
pub const CMD_EQ_WRITE: u16 = 0x2b49;
pub const CMD_EQ_READ: u16 = 0x2b4a;
pub const CMD_LATENCY: u16 = 0x2b6c;
pub const CMD_QUALITY_WRITE: u16 = 0x2ba2;
pub const CMD_QUALITY_READ: u16 = 0x2ba3;

const EQ_PRESETS: &[(i8, &str)] = &[(1, "default"), (2, "hardbass"), (3, "treble"), (9, "voice")];
const TAP_ACTIONS: &[(i8, &str)] = &[
    (-1, "off"),
    (1, "pause"),
    (2, "next"),
    (7, "previous"),
    (0, "assistant"),
];
const LONG_ACTIONS: &[(i8, &str)] = &[
    (-1, "off"),
    (0, "assistant"),
    (18, "volume_up"),
    (19, "volume_down"),
    (10, "noise_cycle"),
];
const SWIPE_ACTIONS: &[(i8, &str)] = &[(-1, "off"), (0, "volume")];
const NOISE_ACTIONS: &[(i8, &str)] = &[
    (1, "off_on"),
    (2, "off_on_aware"),
    (3, "on_aware"),
    (4, "off_aware"),
];

const DUAL_CONNECT: u8 = 1;
const DUAL_DISCONNECT: u8 = 2;
const DUAL_UNPAIR: u8 = 3;
const DUAL_AUTO_ON: u8 = 4;
const DUAL_AUTO_OFF: u8 = 5;

#[derive(Debug, Clone)]
pub enum Request {
    Get,
    Refresh,
    SetAutoPause(bool),
    SetLowLatency(bool),
    SetSoundQuality(String),
    SetEqualizer(String),
    SetGesture {
        gesture: String,
        value: String,
    },
    SetLanguage(String),
    SetDualEnabled(bool),
    Dual {
        action: String,
        mac: String,
        enabled: Option<bool>,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Battery {
    pub left: Option<u8>,
    pub right: Option<u8>,
    /// Case charge. The JSON field stays `case`; `case` is a Rust keyword.
    #[serde(rename = "case")]
    pub case_level: Option<u8>,
    pub global: Option<u8>,
    pub charging: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeviceInfo {
    pub model: Option<String>,
    pub submodel: Option<String>,
    pub hardware: Option<String>,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    pub left_serial: Option<String>,
    pub right_serial: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Gestures {
    pub double_tap_left: Option<String>,
    pub double_tap_right: Option<String>,
    pub triple_tap_left: Option<String>,
    pub triple_tap_right: Option<String>,
    pub long_tap: Option<String>,
    pub swipe: Option<String>,
    pub noise_control: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GestureOptions {
    pub double_tap: Vec<String>,
    pub triple_tap: Vec<String>,
    pub long_tap: Vec<String>,
    pub swipe: Vec<String>,
    pub noise_control: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GestureCodes {
    pub double_tap: Vec<i8>,
    pub triple_tap: Vec<i8>,
    pub long_tap: Vec<i8>,
    pub swipe: Vec<i8>,
    pub noise_control: Vec<i8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub mac: String,
    pub name: String,
    pub connected: bool,
    pub playing: bool,
    pub preferred: bool,
    pub auto_connect: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DualConnect {
    pub enabled: Option<bool>,
    pub preferred: Option<String>,
    pub devices: Vec<Peer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub seq: u64,
    pub phase: String,
    pub address: Option<String>,
    pub name: Option<String>,
    pub paired: bool,
    pub audio_connected: bool,
    pub spp_connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub info: DeviceInfo,
    pub battery: Battery,
    pub wearing: Option<bool>,
    pub auto_pause: Option<bool>,
    pub low_latency: Option<bool>,
    pub sound_quality: Option<String>,
    pub equalizer: Option<String>,
    pub equalizer_options: Vec<String>,
    pub gestures: Gestures,
    pub gesture_options: GestureOptions,
    pub dual_connect: DualConnect,
    pub language: Option<String>,
    pub language_options: Vec<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            kind: "state",
            seq: 0,
            phase: "starting".into(),
            address: None,
            name: None,
            paired: false,
            audio_connected: false,
            spp_connected: false,
            error: None,
            info: DeviceInfo::default(),
            battery: Battery::default(),
            wearing: None,
            auto_pause: None,
            low_latency: None,
            sound_quality: None,
            equalizer: None,
            equalizer_options: eq_names(),
            gestures: Gestures::default(),
            gesture_options: GestureOptions::default(),
            dual_connect: DualConnect::default(),
            language: None,
            language_options: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    pub snap: Snapshot,
    pub codes: GestureCodes,
}

impl Model {
    pub fn new() -> Self {
        Self {
            snap: Snapshot::default(),
            codes: GestureCodes::default(),
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_request(value: &Value) -> Result<Request, String> {
    let cmd = value.get("cmd").and_then(Value::as_str).unwrap_or("");
    match cmd {
        "get" => Ok(Request::Get),
        "refresh" => Ok(Request::Refresh),
        "set" => parse_set(value),
        "dual" => {
            let action = required_str(value, "action")?;
            let mac = required_str(value, "mac")?;
            let enabled = value.get("value").and_then(Value::as_bool);
            Ok(Request::Dual {
                action,
                mac,
                enabled,
            })
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn parse_set(value: &Value) -> Result<Request, String> {
    let key = required_str(value, "key")?;
    match key.as_str() {
        "auto_pause" => Ok(Request::SetAutoPause(required_bool(value)?)),
        "low_latency" => Ok(Request::SetLowLatency(required_bool(value)?)),
        "sound_quality" => Ok(Request::SetSoundQuality(required_str(value, "value")?)),
        "equalizer" => Ok(Request::SetEqualizer(required_str(value, "value")?)),
        "language" => Ok(Request::SetLanguage(required_str(value, "value")?)),
        "dual_connect" => Ok(Request::SetDualEnabled(required_bool(value)?)),
        "gesture" => Ok(Request::SetGesture {
            gesture: value
                .get("gesture")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            value: required_str(value, "value")?,
        }),
        other => Err(format!("unknown setting '{other}'")),
    }
}

fn required_str(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing '{key}'"))
}

fn required_bool(value: &Value) -> Result<bool, String> {
    value
        .get("value")
        .and_then(Value::as_bool)
        .ok_or_else(|| "missing boolean 'value'".into())
}

pub fn ack(ok: bool, error: Option<&str>, snap: &Snapshot) -> Value {
    json!({
        "type": "ack",
        "ok": ok,
        "error": error,
        "state": snap,
    })
}

pub fn battery_query() -> Packet {
    Packet::read(CMD_BATTERY, &[1, 2, 3])
}

pub fn info_query() -> Packet {
    Packet::read(CMD_INFO, &(0..32).collect::<Vec<_>>())
}

pub fn apply_battery(model: &mut Model, packet: &Packet) {
    if let Some(level) = packet.param_u8(1) {
        model.snap.battery.global = Some(level);
    }
    if let Some(levels) = packet.param(2).filter(|levels| levels.len() >= 3) {
        model.snap.battery.left = Some(levels[0]);
        model.snap.battery.right = Some(levels[1]);
        model.snap.battery.case_level = Some(levels[2]);
    }
    if let Some(flags) = packet.param(3) {
        model.snap.battery.charging = Some(flags.contains(&0x01));
    }
}

pub fn apply_info(model: &mut Model, packet: &Packet) {
    for (id, label) in [
        (3u8, "hardware"),
        (7, "firmware"),
        (9, "serial"),
        (10, "submodel"),
        (15, "model"),
    ] {
        if let Some(bytes) = packet.param(id) {
            let text = decode_text(bytes);
            if text.is_empty() {
                continue;
            }
            let slot = match label {
                "hardware" => &mut model.snap.info.hardware,
                "firmware" => &mut model.snap.info.firmware,
                "serial" => &mut model.snap.info.serial,
                "submodel" => &mut model.snap.info.submodel,
                _ => &mut model.snap.info.model,
            };
            *slot = Some(text);
        }
    }
    if let Some(bytes) = packet.param(24) {
        let text = decode_text(bytes);
        if let Some((left, right)) = text.split_once(',') {
            model.snap.info.left_serial = Some(left.trim_start_matches("L-").to_string());
            model.snap.info.right_serial = Some(right.trim_start_matches("R-").to_string());
        }
    }
}

pub fn apply_wearing(model: &mut Model, packet: &Packet) {
    if let Some(value) = packet.param_u8(8).or_else(|| packet.param_u8(9)) {
        model.snap.wearing = Some(value == 1);
    }
}

pub fn apply_auto_pause(model: &mut Model, packet: &Packet) {
    if let Some(value) = packet.param_u8(1) {
        model.snap.auto_pause = Some(value == 1);
    }
}

pub fn apply_latency(model: &mut Model, packet: &Packet) {
    if let Some(value) = packet.param_u8(2).or_else(|| packet.param_u8(1)) {
        model.snap.low_latency = Some(value == 1);
    }
}

pub fn apply_quality(model: &mut Model, packet: &Packet) {
    if let Some(value) = packet.param_i8(2).or_else(|| packet.param_i8(1)) {
        model.snap.sound_quality = Some(match value {
            0 => "connectivity".into(),
            1 => "quality".into(),
            other => format!("code:{other}"),
        });
    }
}

pub fn apply_equalizer(model: &mut Model, packet: &Packet) {
    model.snap.equalizer_options = eq_names();
    if let Some(current) = packet.param_i8(2) {
        model.snap.equalizer = Some(name_for(current, EQ_PRESETS));
        if model
            .snap
            .equalizer
            .as_deref()
            .is_some_and(|name| name.starts_with("code:"))
        {
            model
                .snap
                .equalizer_options
                .push(model.snap.equalizer.clone().unwrap_or_default());
        }
    }
}

pub fn apply_language(model: &mut Model, packet: &Packet) {
    if let Some(bytes) = packet.param(3) {
        let text = decode_text(bytes);
        let options = split_locales(&text);
        if !options.is_empty() {
            model.snap.language_options = options;
        }
    }
    for id in [1u8, 2] {
        if let Some(bytes) = packet.param(id) {
            let text = decode_text(bytes);
            if looks_like_locale(&text) {
                model.snap.language = Some(text);
                break;
            }
        }
    }
}

pub fn apply_tap(model: &mut Model, packet: &Packet, which: TapKind) {
    let (table, codes, options, left, right) = match which {
        TapKind::Double => (
            TAP_ACTIONS,
            &mut model.codes.double_tap,
            &mut model.snap.gesture_options.double_tap,
            &mut model.snap.gestures.double_tap_left,
            &mut model.snap.gestures.double_tap_right,
        ),
        TapKind::Triple => (
            TAP_ACTIONS,
            &mut model.codes.triple_tap,
            &mut model.snap.gesture_options.triple_tap,
            &mut model.snap.gestures.triple_tap_left,
            &mut model.snap.gestures.triple_tap_right,
        ),
    };
    *codes = signed_list(packet.param(3));
    *options = option_names(codes, table);
    if let Some(value) = packet.param_i8(1) {
        *left = Some(name_for(value, table));
    }
    if let Some(value) = packet.param_i8(2) {
        *right = Some(name_for(value, table));
    }
}

pub fn apply_long_tap(model: &mut Model, packet: &Packet) {
    model.codes.long_tap = signed_list(packet.param(3));
    model.snap.gesture_options.long_tap = option_names(&model.codes.long_tap, LONG_ACTIONS);
    if let Some(value) = packet.param_i8(1) {
        model.snap.gestures.long_tap = Some(name_for(value, LONG_ACTIONS));
    }
}

pub fn apply_swipe(model: &mut Model, packet: &Packet) {
    model.codes.swipe = signed_list(packet.param(3));
    model.snap.gesture_options.swipe = option_names(&model.codes.swipe, SWIPE_ACTIONS);
    if let Some(value) = packet.param_i8(1) {
        model.snap.gestures.swipe = Some(name_for(value, SWIPE_ACTIONS));
    }
}

pub fn apply_noise(model: &mut Model, packet: &Packet) {
    model.codes.noise_control = signed_list(packet.param(3));
    model.snap.gesture_options.noise_control =
        option_names(&model.codes.noise_control, NOISE_ACTIONS);
    if let Some(value) = packet.param_i8(1) {
        model.snap.gestures.noise_control = Some(name_for(value, NOISE_ACTIONS));
    }
}

pub fn apply_dual_enabled(model: &mut Model, packet: &Packet) {
    if let Some(value) = packet.param_u8(1) {
        model.snap.dual_connect.enabled = Some(value == 1);
    }
}

#[derive(Debug)]
pub struct DualRow {
    pub index: i16,
    pub expected: i16,
    pub peer: Option<Peer>,
}

pub fn parse_dual_row(packet: &Packet) -> Option<DualRow> {
    if packet.command != CMD_DUAL_ENUMERATE {
        return None;
    }
    let expected = packet.param_i8(2).unwrap_or(0) as i16;
    let index = packet.param_i8(3).unwrap_or(0) as i16;
    let peer = packet.param(4).and_then(format_mac).map(|mac| {
        let state = packet.param_u8(5).unwrap_or(0);
        Peer {
            mac,
            name: packet.param(9).map(decode_text).unwrap_or_default(),
            connected: state > 0,
            playing: state == 9,
            preferred: packet.param_u8(7) == Some(1),
            auto_connect: packet.param_u8(8).map(|value| value == 1),
        }
    });
    Some(DualRow {
        index,
        expected,
        peer,
    })
}

pub fn finish_dual_list(model: &mut Model, peers: Vec<Peer>) {
    let preferred = peers
        .iter()
        .find(|peer| peer.preferred)
        .map(|peer| peer.mac.clone());
    model.snap.dual_connect.devices = peers;
    model.snap.dual_connect.preferred = preferred;
}

pub fn apply_notification(model: &mut Model, packet: &Packet) -> bool {
    match packet.command {
        CMD_BATTERY | CMD_BATTERY_NOTIFY => apply_battery(model, packet),
        CMD_WEARING => apply_wearing(model, packet),
        CMD_AUTO_PAUSE_READ => apply_auto_pause(model, packet),
        CMD_LATENCY => apply_latency(model, packet),
        _ => return false,
    }
    true
}

#[derive(Clone, Copy)]
pub enum TapKind {
    Double,
    Triple,
}

pub fn auto_pause_write(enabled: bool) -> Packet {
    Packet::with_i8(CMD_AUTO_PAUSE_WRITE, &[(1, i8::from(enabled))])
}

pub fn latency_write(enabled: bool) -> Packet {
    Packet::with_i8(CMD_LATENCY, &[(1, i8::from(enabled))])
}

pub fn quality_write(name: &str) -> Result<Packet, String> {
    let code = match name {
        "connectivity" => 0,
        "quality" => 1,
        other => {
            return Err(format!(
                "sound quality '{other}' is not connectivity or quality"
            ));
        }
    };
    Ok(Packet::with_i8(CMD_QUALITY_WRITE, &[(1, code)]))
}

pub fn equalizer_write(name: &str) -> Result<Packet, String> {
    let code = code_for(name, &[], EQ_PRESETS)?;
    Ok(Packet::with_i8(CMD_EQ_WRITE, &[(1, code)]))
}

pub fn gesture_write(model: &Model, gesture: &str, value: &str) -> Result<Packet, String> {
    let (command, param, table, available) = match gesture {
        "double_tap_left" => (
            CMD_DOUBLE_WRITE,
            1u8,
            TAP_ACTIONS,
            model.codes.double_tap.as_slice(),
        ),
        "double_tap_right" => (
            CMD_DOUBLE_WRITE,
            2,
            TAP_ACTIONS,
            model.codes.double_tap.as_slice(),
        ),
        "triple_tap_left" => (
            CMD_TRIPLE_WRITE,
            1,
            TAP_ACTIONS,
            model.codes.triple_tap.as_slice(),
        ),
        "triple_tap_right" => (
            CMD_TRIPLE_WRITE,
            2,
            TAP_ACTIONS,
            model.codes.triple_tap.as_slice(),
        ),
        "long_tap" => (
            CMD_LONG_WRITE,
            1,
            LONG_ACTIONS,
            model.codes.long_tap.as_slice(),
        ),
        "swipe" => {
            let code = code_for(value, &model.codes.swipe, SWIPE_ACTIONS)?;
            return Ok(Packet::with_i8(CMD_SWIPE_WRITE, &[(1, code), (2, code)]));
        }
        "noise_control" => (
            CMD_NOISE_WRITE,
            1,
            NOISE_ACTIONS,
            model.codes.noise_control.as_slice(),
        ),
        other => return Err(format!("unknown gesture '{other}'")),
    };
    let code = code_for(value, available, table)?;
    Ok(Packet::with_i8(command, &[(param, code)]))
}

pub fn language_write(locale: &str) -> Packet {
    Packet::new(
        CMD_LANGUAGE_WRITE,
        vec![(1, locale.as_bytes().to_vec()), (2, vec![1])],
    )
}

pub fn dual_enabled_write(enabled: bool) -> Packet {
    Packet::with_i8(CMD_DUAL_ENABLED_WRITE, &[(1, i8::from(enabled))])
}

pub fn dual_prefer(mac: &str) -> Result<Packet, String> {
    Ok(Packet::new(
        CMD_DUAL_PREFER,
        vec![(1, parse_mac(mac)?.to_vec())],
    ))
}

pub fn dual_execute(action: &str, mac: &str, enabled: Option<bool>) -> Result<Packet, String> {
    let code = match action {
        "connect" => DUAL_CONNECT,
        "disconnect" => DUAL_DISCONNECT,
        "unpair" => DUAL_UNPAIR,
        "auto" => {
            if enabled.unwrap_or(false) {
                DUAL_AUTO_ON
            } else {
                DUAL_AUTO_OFF
            }
        }
        "prefer" => return dual_prefer(mac),
        other => return Err(format!("unknown dual-device action '{other}'")),
    };
    Ok(Packet::new(
        CMD_DUAL_EXECUTE,
        vec![(code, parse_mac(mac)?.to_vec())],
    ))
}

pub fn same_state(left: &Snapshot, right: &Snapshot) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.seq = 0;
    right.seq = 0;
    serde_json::to_string(&left).ok() == serde_json::to_string(&right).ok()
}

fn eq_names() -> Vec<String> {
    EQ_PRESETS
        .iter()
        .map(|(_, name)| (*name).to_string())
        .collect()
}

fn name_for(code: i8, table: &[(i8, &str)]) -> String {
    table
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| format!("code:{code}"))
}

fn option_names(codes: &[i8], table: &[(i8, &str)]) -> Vec<String> {
    if codes.is_empty() {
        return table.iter().map(|(_, name)| (*name).to_string()).collect();
    }
    let mut names = Vec::new();
    for code in codes {
        let name = name_for(*code, table);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn code_for(name: &str, available: &[i8], table: &[(i8, &str)]) -> Result<i8, String> {
    if let Some(raw) = name.strip_prefix("code:") {
        return raw
            .parse::<i8>()
            .map_err(|_| format!("action '{name}' is not a code"));
    }
    if !available.is_empty() {
        for code in available {
            if table
                .iter()
                .any(|(candidate, label)| candidate == code && *label == name)
            {
                return Ok(*code);
            }
        }
    }
    table
        .iter()
        .find(|(_, label)| *label == name)
        .map(|(code, _)| *code)
        .ok_or_else(|| format!("action '{name}' is not supported"))
}

fn signed_list(bytes: Option<&[u8]>) -> Vec<i8> {
    bytes
        .map(|bytes| bytes.iter().map(|byte| *byte as i8).collect())
        .unwrap_or_default()
}

fn decode_text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn split_locales(text: &str) -> Vec<String> {
    text.split([',', ';'])
        .map(str::trim)
        .filter(|item| looks_like_locale(item))
        .map(str::to_string)
        .collect()
}

fn looks_like_locale(text: &str) -> bool {
    let len = text.chars().count();
    (2..=16).contains(&len)
        && text.chars().any(|ch| ch.is_ascii_alphabetic())
        && !text.contains(' ')
}

pub fn format_mac(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 6 {
        return None;
    }
    Some(
        bytes
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

pub fn parse_mac(text: &str) -> Result<[u8; 6], String> {
    let hex: String = text.chars().filter(|ch| ch.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return Err(format!("'{text}' is not a 6-byte bluetooth address"));
    }
    let mut out = [0u8; 6];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|err| err.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spp::Packet;

    fn decode(hex_text: &str) -> Packet {
        let bytes = (0..hex_text.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex_text[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>();
        Packet::decode(&bytes).unwrap()
    }

    #[test]
    fn battery_levels_come_from_the_three_parameter() {
        let mut model = Model::new();
        apply_battery(
            &mut model,
            &decode("5a0014000108010140020310203003030001000402140a1461"),
        );
        assert_eq!(model.snap.battery.global, Some(0x40));
        assert_eq!(model.snap.battery.left, Some(0x10));
        assert_eq!(model.snap.battery.right, Some(0x20));
        assert_eq!(model.snap.battery.case_level, Some(0x30));
        let json = serde_json::to_string(&model.snap.battery).unwrap();
        assert!(json.contains("\"case\":48"), "{json}");
        assert_eq!(model.snap.battery.charging, Some(true));
    }

    #[test]
    fn double_tap_uses_device_codes() {
        let mut model = Model::new();
        apply_tap(
            &mut model,
            &decode("5a0017000120010101020102030501070200ff0401ff060200ffcf60"),
            TapKind::Double,
        );
        assert_eq!(
            model.snap.gestures.double_tap_left.as_deref(),
            Some("pause")
        );
        assert_eq!(
            model.snap.gestures.double_tap_right.as_deref(),
            Some("next")
        );
        assert_eq!(
            model.snap.gesture_options.double_tap,
            vec!["pause", "previous", "next", "assistant", "off"]
        );
        let write = gesture_write(&model, "double_tap_left", "next").unwrap();
        assert_eq!(write.param_i8(1), Some(2));
    }

    #[test]
    fn swipe_off_writes_both_sides() {
        let model = Model::new();
        let packet = gesture_write(&model, "swipe", "off").unwrap();
        assert_eq!(packet.command, CMD_SWIPE_WRITE);
        assert_eq!(packet.param_i8(1), Some(-1));
        assert_eq!(packet.param_i8(2), Some(-1));
    }

    #[test]
    fn requests_accept_plugin_json() {
        let refresh = parse_request(&json!({"cmd": "refresh"})).unwrap();
        assert!(matches!(refresh, Request::Refresh));
        let latency =
            parse_request(&json!({"cmd": "set", "key": "low_latency", "value": true})).unwrap();
        assert!(matches!(latency, Request::SetLowLatency(true)));
        let gesture = parse_request(&json!({
            "cmd": "set",
            "key": "gesture",
            "gesture": "long_tap",
            "value": "volume_up"
        }))
        .unwrap();
        match gesture {
            Request::SetGesture { gesture, value } => {
                assert_eq!(gesture, "long_tap");
                assert_eq!(value, "volume_up");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
