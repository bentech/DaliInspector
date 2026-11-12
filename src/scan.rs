use crate::short_query_address;
use serde_json::{json, Map, Value};

pub trait QueryBus {
    /// Send one 16-bit DALI frame. A response is Some(byte); no answer is None.
    fn transact(&mut self, address: u8, command: u8) -> Result<Option<u8>, String>;
}

const QUERIES: &[(u8, &str)] = &[
    (0x90, "status"),
    (0x92, "lamp_failure"),
    (0x93, "lamp_power_on"),
    (0x94, "limit_error"),
    (0x95, "reset_state"),
    (0x96, "missing_short_address"),
    (0x97, "version_number"),
    (0x99, "device_type"),
    (0x9a, "physical_minimum"),
    (0x9b, "power_failure"),
    (0x9e, "operating_mode"),
    (0x9f, "light_source_type"),
    (0xa0, "actual_level"),
    (0xa1, "max_level"),
    (0xa2, "min_level"),
    (0xa3, "power_on_level"),
    (0xa4, "system_failure_level"),
    (0xa5, "fade_time_and_rate"),
    (0xa6, "manufacturer_specific_mode"),
    (0xa8, "extended_fade_time"),
    (0xaa, "control_gear_failure"),
    (0xc0, "groups_0_to_7"),
    (0xc1, "groups_8_to_15"),
    (0xc2, "random_address_high"),
    (0xc3, "random_address_mid"),
    (0xc4, "random_address_low"),
];

fn ask<B: QueryBus>(bus: &mut B, address: u8, command: u8) -> Result<Option<u8>, String> {
    bus.transact(address, command)
        .map_err(|e| format!("DALI {:02X} {:02X}: {e}", address, command))
}

pub fn inspect<B: QueryBus>(bus: &mut B, short_address: u8) -> Result<Option<Value>, String> {
    let address = short_query_address(short_address).ok_or("short address must be 0..63")?;
    if ask(bus, address, 0x91)?.is_none() {
        return Ok(None);
    }

    let mut query_values = Map::new();
    for &(command, name) in QUERIES {
        let value = ask(bus, address, command)?;
        query_values.insert(name.into(), json!(value));
        if command == 0x99 && value == Some(0xff) {
            let mut types = Vec::new();
            for _ in 0..32 {
                match ask(bus, address, 0xa7)? {
                    Some(254) | None => break,
                    Some(device_type) => types.push(device_type),
                }
            }
            query_values.insert("device_types".into(), json!(types));
        }
    }
    let value = |key: &str| {
        query_values
            .get(key)
            .and_then(Value::as_u64)
            .map(|n| n as u8)
    };
    let groups_mask = value("groups_0_to_7")
        .map(u16::from)
        .zip(value("groups_8_to_15"))
        .map(|(lo, hi)| lo | (u16::from(hi) << 8));
    let groups = groups_mask.map(|mask| {
        (0..16)
            .filter(|bit| mask & (1 << bit) != 0)
            .collect::<Vec<u8>>()
    });
    let random_address = value("random_address_high")
        .zip(value("random_address_mid"))
        .zip(value("random_address_low"))
        .map(|((hi, mid), lo)| format!("{hi:02X}{mid:02X}{lo:02X}"));
    let fade = value("fade_time_and_rate");
    let status = value("status");
    let scenes = (0..16)
        .map(|scene| {
            let level = ask(bus, address, 0xb0 + scene)?;
            Ok(json!({"scene": scene, "level": level.filter(|v| *v != 0xff)}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let memory_bank_0 = read_memory_bank_0(bus, address, value("version_number"))?;

    Ok(Some(json!({
        "short_address": short_address,
        "queries": query_values,
        "status_flags": status.map(|s| json!({
            "control_gear_failure": s & 0x01 != 0,
            "lamp_failure": s & 0x02 != 0,
            "lamp_power_on": s & 0x04 != 0,
            "limit_error": s & 0x08 != 0,
            "fade_running": s & 0x10 != 0,
            "reset_state": s & 0x20 != 0,
            "missing_short_address": s & 0x40 != 0,
            "power_failure": s & 0x80 != 0
        })),
        "fade_time_code": fade.map(|v| v >> 4),
        "fade_rate_code": fade.map(|v| v & 0x0f),
        "groups": groups,
        "scenes": scenes,
        "random_address": random_address,
        "memory_bank_0": memory_bank_0
    })))
}

fn read_memory_bank_0<B: QueryBus>(
    bus: &mut B,
    address: u8,
    version: Option<u8>,
) -> Result<Option<Value>, String> {
    ask(bus, 0xc3, 0)?; // Broadcast DTR1 = bank 0.
    ask(bus, 0xa3, 0)?; // Broadcast DTR0 = location 0.
    let mut bytes = Vec::new();
    let limit = if version == Some(1) { 0x0e } else { 0x1a };
    for _ in 0..=limit {
        match ask(bus, address, 0xc5)? {
            Some(byte) => bytes.push(byte),
            None => break,
        }
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    let slice_hex = |start: usize, end: usize| {
        bytes
            .get(start..=end)
            .map(|slice| slice.iter().map(|b| format!("{b:02X}")).collect::<String>())
    };
    let version_byte = |index: usize| {
        bytes.get(index).map(|b| {
            if *b == 0xff {
                "not implemented".to_string()
            } else {
                format!("{}.{}", b >> 2, b & 3)
            }
        })
    };
    Ok(Some(json!({
        "raw_bytes": bytes,
        "last_address": bytes.first(),
        "last_memory_bank": bytes.get(2),
        "gtin_hex": slice_hex(3, 8),
        "firmware_version": bytes.get(9).zip(bytes.get(10)).map(|(major, minor)| format!("{major}.{minor}")),
        "identification_number_hex": slice_hex(11, if version == Some(1) { 14 } else { 18 }),
        "hardware_version": bytes.get(19).zip(bytes.get(20)).map(|(major, minor)| format!("{major}.{minor}")),
        "part_101_version": version_byte(21),
        "part_102_version": version_byte(22),
        "part_103_version": version_byte(23),
        "control_device_units": bytes.get(24),
        "control_gear_units": bytes.get(25),
        "unit_index": bytes.get(26)
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct MockBus(VecDeque<(u8, u8, Option<u8>)>);
    impl QueryBus for MockBus {
        fn transact(&mut self, address: u8, command: u8) -> Result<Option<u8>, String> {
            let (expected_address, expected_command, response) =
                self.0.pop_front().ok_or("unexpected query")?;
            assert_eq!((address, command), (expected_address, expected_command));
            Ok(response)
        }
    }

    #[test]
    fn absent_address_is_omitted() {
        let mut bus = MockBus(VecDeque::from([(0x03, 0x91, None)]));
        assert_eq!(inspect(&mut bus, 1).unwrap(), None);
    }

    #[test]
    fn present_address_has_machine_readable_fields() {
        let mut replies = VecDeque::from([(0x03, 0x91, Some(0xff))]);
        for &(command, _) in QUERIES {
            let response = match command {
                0x90 => Some(0x04),
                0x99 => Some(6),
                0xa0 => Some(128),
                0xa5 => Some(0x57),
                0xc0 => Some(0b0000_0010),
                0xc1 => Some(0),
                _ => None,
            };
            replies.push_back((0x03, command, response));
        }
        for scene in 0..16 {
            replies.push_back((0x03, 0xb0 + scene, Some(if scene == 1 { 42 } else { 255 })));
        }
        replies.push_back((0xc3, 0, None));
        replies.push_back((0xa3, 0, None));
        replies.push_back((0x03, 0xc5, None));
        let result = inspect(&mut MockBus(replies), 1).unwrap().unwrap();
        assert_eq!(result["queries"]["actual_level"], 128);
        assert_eq!(result["groups"], json!([1]));
        assert_eq!(result["scenes"][1]["level"], 42);
        assert_eq!(result["fade_time_code"], 5);
        assert_eq!(result["memory_bank_0"], Value::Null);
    }
}
