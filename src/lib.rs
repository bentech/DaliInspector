//! Passive decoding of the older Lunatone/Tridonic DALI USB HID reports.

pub const VENDOR_ID: u16 = 0x17b5;
pub const PRODUCT_ID: u16 = 0x0020;
pub mod control;
pub mod scan;

/// Build one 64-byte USB interrupt output payload. HID APIs prepend a zero
/// report ID to this payload when no numbered reports are in use.
pub fn query_payload(sequence: u8, address: u8, command: u8) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    bytes[..8].copy_from_slice(&[0x12, sequence, 0, 0x03, 0, 0, address, command]);
    bytes
}

pub fn short_query_address(short_address: u8) -> Option<u8> {
    (short_address < 64).then_some((short_address << 1) | 1)
}

pub fn command_name(address: u8, command: u8) -> String {
    if address <= 0x9f || matches!(address, 0xfc | 0xfd | 0xfe | 0xff) {
        if address & 1 == 0 {
            return format!("DAPC level={command}");
        }
        return match command {
            0x00 => "OFF".into(),
            0x01 => "UP".into(),
            0x02 => "DOWN".into(),
            0x03 => "STEP UP".into(),
            0x04 => "STEP DOWN".into(),
            0x05 => "RECALL MAX LEVEL".into(),
            0x06 => "RECALL MIN LEVEL".into(),
            0x07 => "STEP DOWN AND OFF".into(),
            0x08 => "ON AND STEP UP".into(),
            0x09 => "ENABLE DAPC SEQUENCE".into(),
            0x0a => "GO TO LAST ACTIVE LEVEL".into(),
            0x10..=0x1f => format!("GO TO SCENE {}", command - 0x10),
            0x20 => "RESET".into(),
            0x21 => "STORE ACTUAL LEVEL IN DTR0".into(),
            0x2a => "SET MAX LEVEL".into(),
            0x2b => "SET MIN LEVEL".into(),
            0x2c => "SET SYSTEM FAILURE LEVEL".into(),
            0x2d => "SET POWER ON LEVEL".into(),
            0x2e => "SET FADE TIME".into(),
            0x2f => "SET FADE RATE".into(),
            0x30 => "SET EXTENDED FADE TIME".into(),
            0x40..=0x4f => format!("SET SCENE {}", command - 0x40),
            0x50..=0x5f => format!("REMOVE FROM SCENE {}", command - 0x50),
            0x60..=0x6f => format!("ADD TO GROUP {}", command - 0x60),
            0x70..=0x7f => format!("REMOVE FROM GROUP {}", command - 0x70),
            0x80 => "SET SHORT ADDRESS".into(),
            0x81 => "ENABLE WRITE MEMORY".into(),
            0x90 => "QUERY STATUS".into(),
            0x91 => "QUERY CONTROL GEAR PRESENT".into(),
            0x92 => "QUERY LAMP FAILURE".into(),
            0x93 => "QUERY LAMP POWER ON".into(),
            0x94 => "QUERY LIMIT ERROR".into(),
            0x95 => "QUERY RESET STATE".into(),
            0x96 => "QUERY MISSING SHORT ADDRESS".into(),
            0x97 => "QUERY VERSION NUMBER".into(),
            0x98 => "QUERY DTR0".into(),
            0x99 => "QUERY DEVICE TYPE".into(),
            0x9a => "QUERY PHYSICAL MINIMUM".into(),
            0x9b => "QUERY POWER FAILURE".into(),
            0x9c => "QUERY DTR1".into(),
            0x9d => "QUERY DTR2".into(),
            0x9e => "QUERY OPERATING MODE".into(),
            0x9f => "QUERY LIGHT SOURCE TYPE".into(),
            0xa0 => "QUERY ACTUAL LEVEL".into(),
            0xa1 => "QUERY MAX LEVEL".into(),
            0xa2 => "QUERY MIN LEVEL".into(),
            0xa3 => "QUERY POWER ON LEVEL".into(),
            0xa4 => "QUERY SYSTEM FAILURE LEVEL".into(),
            0xa5 => "QUERY FADE TIME AND RATE".into(),
            0xa6 => "QUERY MANUFACTURER SPECIFIC MODE".into(),
            0xa7 => "QUERY NEXT DEVICE TYPE".into(),
            0xa8 => "QUERY EXTENDED FADE TIME".into(),
            0xaa => "QUERY CONTROL GEAR FAILURE".into(),
            0xb0..=0xbf => format!("QUERY SCENE {} LEVEL", command - 0xb0),
            0xc0 => "QUERY GROUPS 0-7".into(),
            0xc1 => "QUERY GROUPS 8-15".into(),
            0xc2 => "QUERY RANDOM ADDRESS HIGH".into(),
            0xc3 => "QUERY RANDOM ADDRESS MID".into(),
            0xc4 => "QUERY RANDOM ADDRESS LOW".into(),
            0xc5 => "READ MEMORY LOCATION".into(),
            0xe0..=0xfe => format!("DEVICE TYPE COMMAND 0x{command:02X}"),
            0xff => "QUERY EXTENDED VERSION".into(),
            _ => format!("COMMAND 0x{command:02X}"),
        };
    }
    match address {
        0xa1 => "TERMINATE".into(),
        0xa3 => format!("SET DTR0={command}"),
        0xa5 => "INITIALISE".into(),
        0xa7 => "RANDOMISE".into(),
        0xa9 => format!("COMPARE parameter={command}"),
        0xb9 => format!("VERIFY SHORT ADDRESS byte={command:02X}"),
        0xbb => "QUERY SHORT ADDRESS".into(),
        0xc1 => format!("ENABLE DEVICE TYPE {command}"),
        0xc3 => format!("SET DTR1={command}"),
        0xc5 => format!("SET DTR2={command}"),
        _ => format!("SPECIAL 0x{address:02X} 0x{command:02X}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    Forward16 {
        address: u8,
        data: u8,
        status: [u8; 2],
    },
    Forward24 {
        bytes: [u8; 3],
        status: [u8; 2],
    },
    BusAnswer {
        value: u8,
        status: [u8; 2],
    },
    BusNoAnswer {
        status: [u8; 2],
    },
    Answer {
        value: u8,
        sequence: u8,
        status: [u8; 2],
    },
    NoAnswer {
        sequence: u8,
        status: [u8; 2],
    },
    HostComplete {
        bytes: [u8; 3],
        sequence: u8,
        status: [u8; 2],
    },
    Unknown {
        direction: u8,
        kind: u8,
    },
}

/// HID implementations may prepend report ID zero. Keep the original bytes
/// separately when logging; this only selects the protocol payload.
pub fn payload(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= 10 && bytes[0] == 0 && matches!(bytes[1], 0x11 | 0x12) {
        &bytes[1..]
    } else {
        bytes
    }
}

pub fn decode(bytes: &[u8]) -> Result<Report, &'static str> {
    let data = payload(bytes);
    if data.len() < 9 {
        return Err("report shorter than 9 byte header");
    }
    let status = [data[6], data[7]];
    let sequence = data[8];
    Ok(match (data[0], data[1]) {
        (0x11, 0x73) => Report::Forward16 {
            address: data[4],
            data: data[5],
            status,
        },
        (0x11, 0x74) => Report::Forward24 {
            bytes: [data[3], data[4], data[5]],
            status,
        },
        (0x11, 0x72) => Report::BusAnswer {
            value: data[5],
            status,
        },
        (0x11, 0x71) => Report::BusNoAnswer { status },
        (0x12, 0x72) => Report::Answer {
            value: data[5],
            sequence,
            status,
        },
        (0x12, 0x71) => Report::NoAnswer { sequence, status },
        (0x12, 0x73) => Report::HostComplete {
            bytes: [data[3], data[4], data[5]],
            sequence,
            status,
        },
        (direction, kind) => Report::Unknown { direction, kind },
    })
}

pub fn describe(report: Report) -> String {
    match report {
        Report::Forward16 {
            address,
            data,
            status,
        } => format!(
            "bus 16-bit {:02X} {:02X} {} {} status={:02X}{:02X}",
            address,
            data,
            address_name(address),
            command_name(address, data),
            status[0],
            status[1]
        ),
        Report::Forward24 { bytes, status } => format!(
            "bus 24-bit {:02X} {:02X} {:02X} status={:02X}{:02X}",
            bytes[0], bytes[1], bytes[2], status[0], status[1]
        ),
        Report::BusAnswer { value, status } => format!(
            "bus answer {:02X} status={:02X}{:02X}",
            value, status[0], status[1]
        ),
        Report::BusNoAnswer { status } => {
            format!("bus no-answer status={:02X}{:02X}", status[0], status[1])
        }
        Report::Answer {
            value,
            sequence,
            status,
        } => format!(
            "host answer {:02X} seq={} status={:02X}{:02X}",
            value, sequence, status[0], status[1]
        ),
        Report::NoAnswer { sequence, status } => format!(
            "host no-answer seq={} status={:02X}{:02X}",
            sequence, status[0], status[1]
        ),
        Report::HostComplete {
            bytes,
            sequence,
            status,
        } => format!(
            "host complete {:02X} {:02X} {:02X} seq={} status={:02X}{:02X}",
            bytes[0], bytes[1], bytes[2], sequence, status[0], status[1]
        ),
        Report::Unknown { direction, kind } => {
            format!("unknown direction={direction:02X} type={kind:02X}")
        }
    }
}

fn address_name(address: u8) -> String {
    if matches!(address, 0xfc | 0xfd | 0xfe | 0xff) {
        "broadcast".to_string()
    } else if (address & 0xe0) == 0x80 {
        format!("group={}", (address >> 1) & 0x0f)
    } else if address <= 0x7f {
        format!("short={}", address >> 1)
    } else {
        "special".to_string()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_query(command: u8) -> bool {
    matches!(command, 0x90..=0x9f | 0xa0..=0xaa | 0xb0..=0xbf | 0xc0..=0xc5 | 0xff)
}

pub fn describe_answer(command: u8, value: u8) -> String {
    match command {
        0x90 => format!(
            "status gear_failure={} lamp_failure={} lamp_on={} limit_error={} fade_running={} reset={} missing_address={} power_failure={}",
            value & 1 != 0, value & 2 != 0, value & 4 != 0, value & 8 != 0,
            value & 16 != 0, value & 32 != 0, value & 64 != 0, value & 128 != 0
        ),
        0x91..=0x96 | 0x9b | 0xa6 | 0xaa => format!("{}", if value == 0xff { "YES" } else { "NO/0x00" }),
        0x99 | 0xa7 => format!("device_type={value}"),
        0xa5 => format!("fade_time_code={} fade_rate_code={}", value >> 4, value & 15),
        0xc0 => format!("groups_0_to_7_mask=0x{value:02X}"),
        0xc1 => format!("groups_8_to_15_mask=0x{value:02X}"),
        0xb0..=0xbf if value == 0xff => "scene_not_set".into(),
        _ => format!("value={value}"),
    }
}

pub fn parse_hex_line(line: &str) -> Result<Vec<u8>, String> {
    let before_comment = line.split('#').next().unwrap_or("");
    let mut result = Vec::new();
    for token in before_comment.split_whitespace() {
        let token = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        if token.len() != 2 {
            return Err(format!("expected two hex digits, got {token:?}"));
        }
        result.push(
            u8::from_str_radix(token, 16).map_err(|_| format!("invalid hex byte {token:?}"))?,
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_known_bus_reports() {
        assert_eq!(
            decode(&[0x11, 0x73, 0, 0, 0xff, 0x93, 0xff, 0xff, 0]).unwrap(),
            Report::Forward16 {
                address: 0xff,
                data: 0x93,
                status: [0xff, 0xff]
            }
        );
        assert_eq!(
            decode(&[0, 0x11, 0x74, 0, 0x07, 0x01, 0xd5, 0, 0, 0]).unwrap(),
            Report::Forward24 {
                bytes: [0x07, 0x01, 0xd5],
                status: [0, 0]
            }
        );
    }

    #[test]
    fn preserves_unknown_types() {
        assert_eq!(
            decode(&[0x11, 0x77, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            Report::Unknown {
                direction: 0x11,
                kind: 0x77
            }
        );
    }

    #[test]
    fn parses_replay_lines() {
        assert_eq!(
            parse_hex_line("11 73 00 00 FF 93 FF FF 00 # sample")
                .unwrap()
                .len(),
            9
        );
        assert!(parse_hex_line("11 ZZ").is_err());
    }

    #[test]
    fn builds_addressed_query() {
        assert_eq!(short_query_address(63), Some(0x7f));
        assert_eq!(short_query_address(64), None);
        assert_eq!(
            &query_payload(7, 0x7f, 0x91)[..8],
            &[0x12, 7, 0, 3, 0, 0, 0x7f, 0x91]
        );
        assert_eq!(command_name(0x7f, 0x91), "QUERY CONTROL GEAR PRESENT");
    }

    #[test]
    fn decodes_bus_answer_and_status() {
        assert_eq!(
            decode(&[0x11, 0x72, 0, 0, 0, 0x84, 0, 0, 0]).unwrap(),
            Report::BusAnswer {
                value: 0x84,
                status: [0, 0]
            }
        );
        assert!(describe_answer(0x90, 0x84).contains("lamp_on=true"));
    }
}
