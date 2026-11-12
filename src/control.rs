//! DALI control frames for short-addressed control gear.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub address: u8,
    pub data: u8,
    pub label: &'static str,
}

fn frame(address: u8, data: u8, label: &'static str) -> Frame {
    Frame {
        address,
        data,
        label,
    }
}

fn number(value: &str, name: &str, max: u16) -> Result<u8, String> {
    let n: u16 = value
        .parse()
        .map_err(|_| format!("{name} must be 0..{max}"))?;
    if n > max {
        return Err(format!("{name} must be 0..{max}"));
    }
    Ok(n as u8)
}

fn level(value: &str, mask: bool) -> Result<u8, String> {
    if mask && value.eq_ignore_ascii_case("mask") {
        Ok(255)
    } else {
        number(value, "level", 254)
    }
}

fn short(value: &str) -> Result<u8, String> {
    number(value, "short address", 63)
}

fn scene(value: &str) -> Result<u8, String> {
    let n: u8 = value.parse().map_err(|_| "scene must be 1..16")?;
    if !(1..=16).contains(&n) {
        return Err("scene must be 1..16".into());
    }
    Ok(n - 1)
}

fn dtr0(out: &mut Vec<Frame>, value: u8) {
    out.push(frame(0xa3, value, "DTR0"));
}
fn dtr1(out: &mut Vec<Frame>, value: u8) {
    out.push(frame(0xc3, value, "DTR1"));
}
fn dtr2(out: &mut Vec<Frame>, value: u8) {
    out.push(frame(0xc5, value, "DTR2"));
}
fn enable(out: &mut Vec<Frame>, device_type: u8) {
    out.push(frame(0xc1, device_type, "ENABLE DEVICE TYPE"));
}
fn twice(out: &mut Vec<Frame>, address: u8, data: u8, label: &'static str) {
    out.push(frame(address, data, label));
    out.push(frame(address, data, label));
}
fn word(out: &mut Vec<Frame>, value: u16) {
    dtr0(out, value as u8);
    dtr1(out, (value >> 8) as u8);
}
fn dt8(out: &mut Vec<Frame>, address: u8, data: u8, label: &'static str) {
    enable(out, 8);
    out.push(frame(address, data, label));
}

fn colour(
    out: &mut Vec<Frame>,
    address: u8,
    kind: &str,
    values: &[String],
    rgb_mode: &str,
) -> Result<(), String> {
    match kind {
        "ct" if values.len() == 1 => {
            let kelvin: u32 = values[0]
                .parse()
                .map_err(|_| "CT must be Kelvin (16..1000000)")?;
            if !(16..=1_000_000).contains(&kelvin) {
                return Err("CT must be Kelvin (16..1000000)".into());
            }
            let mirek = 1_000_000 / kelvin;
            if !(1..=65534).contains(&mirek) {
                return Err("CT is outside the DALI range".into());
            }
            word(out, mirek as u16);
            dt8(out, address, 0xe7, "SET TEMPORARY CT");
        }
        "xy" if values.len() == 2 => {
            let mut sum = 0.0;
            for (value, command, label) in [
                (&values[0], 0xe0, "SET TEMPORARY X"),
                (&values[1], 0xe1, "SET TEMPORARY Y"),
            ] {
                let coordinate: f64 = value
                    .parse()
                    .map_err(|_| "xy coordinates must be 0.0..1.0")?;
                if !coordinate.is_finite() || !(0.0..=65534.0 / 65536.0).contains(&coordinate) {
                    return Err("xy coordinates must be 0.0..0.99997".into());
                }
                sum += coordinate;
                word(out, (coordinate * 65536.0).round() as u16);
                dt8(out, address, command, label);
            }
            if sum > 1.0 {
                return Err("xy coordinates must sum to at most 1.0".into());
            }
        }
        "rgb" if values.len() == 3 => rgb(out, address, values, false, rgb_mode)?,
        "rgbwaf" if values.len() == 6 => rgb(out, address, values, true, rgb_mode)?,
        _ => {
            return Err("colour format: ct KELVIN | xy X Y | rgb R G B | rgbwaf R G B W A F".into())
        }
    }
    Ok(())
}

fn rgb(
    out: &mut Vec<Frame>,
    address: u8,
    values: &[String],
    waf: bool,
    rgb_mode: &str,
) -> Result<(), String> {
    let mut channels = Vec::new();
    for value in values {
        channels.push(number(value, "RGBWAF channel", 254)?);
    }
    dtr0(out, channels[0]);
    dtr1(out, channels[1]);
    dtr2(out, channels[2]);
    dt8(out, address, 0xeb, "SET TEMPORARY RGB");
    if waf {
        dtr0(out, channels[3]);
        dtr1(out, channels[4]);
        dtr2(out, channels[5]);
        dt8(out, address, 0xec, "SET TEMPORARY WAF");
    }
    let control = match rgb_mode {
        "normalised" => 0x80,
        "extended" => 0xc0,
        "legacy" => {
            if waf {
                0x3f
            } else {
                0x07
            }
        }
        _ => return Err("--rgb-mode must be normalised, extended or legacy".into()),
    };
    dtr0(out, control);
    dt8(out, address, 0xed, "SET TEMPORARY RGBWAF CONTROL");
    Ok(())
}

/// Build the exact forward frames to send. Scenes on the CLI are 1..16.
pub fn plan(args: &[String]) -> Result<Vec<Frame>, String> {
    plan_with_rgb_mode(args, "normalised")
}

pub fn plan_with_rgb_mode(args: &[String], rgb_mode: &str) -> Result<Vec<Frame>, String> {
    if args.len() < 3 {
        return Err("send/set/write-scene needs a short address and value".into());
    }
    let address = short(&args[1])?;
    let command_address = (address << 1) | 1;
    let arc_address = address << 1;
    let mut out = Vec::new();
    match args[0].as_str() {
        "send" => match args[2].as_str() {
            "level" | "intensity" if args.len() == 4 => out.push(frame(arc_address, level(&args[3], true)?, "DIRECT ARC POWER")),
            "scene" if args.len() == 4 => out.push(frame(command_address, 0x10 + scene(&args[3])?, "GO TO SCENE")),
            kind @ ("ct" | "xy" | "rgb" | "rgbwaf") => {
                colour(&mut out, command_address, kind, &args[3..], rgb_mode)?;
                dt8(&mut out, command_address, 0xe2, "ACTIVATE COLOUR");
            }
            _ => return Err("send SHORT level N | scene 1..16 | ct KELVIN | xy X Y | rgb R G B | rgbwaf R G B W A F".into()),
        },
        "set" if args.len() == 4 => {
            let (code, value, label) = match args[2].as_str() {
                "fade-time" => (0x2e, number(&args[3], "fade time code", 15)?, "SET FADE TIME"),
                "fade-rate" => (0x2f, number(&args[3], "fade rate code", 15)?, "SET FADE RATE"),
                "extended-fade-time" => {
                    let v = number(&args[3], "extended fade code", 79)?;
                    (0x30, v, "SET EXTENDED FADE TIME")
                }
                "min-level" => (0x2b, level(&args[3], false)?, "SET MIN LEVEL"),
                "max-level" => (0x2a, level(&args[3], false)?, "SET MAX LEVEL"),
                "power-on-level" => (0x2d, level(&args[3], true)?, "SET POWER ON LEVEL"),
                "system-failure-level" => (0x2c, level(&args[3], true)?, "SET SYSTEM FAILURE LEVEL"),
                "led-curve" => {
                    let curve = match args[3].as_str() { "exponential" | "standard" => 0, "linear" => 1, _ => return Err("LED curve must be linear or exponential".into()) };
                    dtr0(&mut out, curve);
                    enable(&mut out, 6);
                    twice(&mut out, command_address, 0xe3, "SELECT DIMMING CURVE");
                    return Ok(out);
                }
                _ => return Err("unknown setting".into()),
            };
            dtr0(&mut out, value);
            twice(&mut out, command_address, code, label);
        }
        "write-scene" if args.len() >= 4 => {
            let native_scene = scene(&args[2])?;
            let scene_level = level(&args[3], true)?;
            if args.len() > 4 { colour(&mut out, command_address, &args[4], &args[5..], rgb_mode)?; }
            dtr0(&mut out, scene_level);
            twice(&mut out, command_address, 0x40 + native_scene, "STORE SCENE");
        }
        _ => return Err("unknown control command or wrong number of values".into()),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(words: &[&str]) -> Vec<Frame> {
        plan(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }
    fn bytes(frames: &[Frame]) -> Vec<(u8, u8)> {
        frames.iter().map(|f| (f.address, f.data)).collect()
    }
    #[test]
    fn direct_and_scene() {
        assert_eq!(
            bytes(&p(&["send", "12", "level", "127"])),
            vec![(0x18, 127)]
        );
        assert_eq!(bytes(&p(&["send", "12", "scene", "1"])), vec![(0x19, 0x10)]);
        assert_eq!(
            bytes(&p(&["send", "12", "scene", "16"])),
            vec![(0x19, 0x1f)]
        );
    }
    #[test]
    fn settings_repeat() {
        assert_eq!(
            bytes(&p(&["set", "2", "fade-time", "0"])),
            vec![(0xa3, 0), (5, 0x2e), (5, 0x2e)]
        );
        assert_eq!(
            bytes(&p(&["set", "2", "led-curve", "linear"])),
            vec![(0xa3, 1), (0xc1, 6), (5, 0xe3), (5, 0xe3)]
        );
        assert_eq!(
            bytes(&p(&["set", "2", "power-on-level", "mask"])),
            vec![(0xa3, 255), (5, 0x2d), (5, 0x2d)]
        );
        for (name, value, opcode, dtr) in [
            ("extended-fade-time", "16", 0x30, 16),
            ("fade-rate", "7", 0x2f, 7),
            ("min-level", "5", 0x2b, 5),
            ("max-level", "254", 0x2a, 254),
            ("system-failure-level", "mask", 0x2c, 255),
        ] {
            assert_eq!(
                bytes(&p(&["set", "2", name, value])),
                vec![(0xa3, dtr), (5, opcode), (5, opcode)]
            );
        }
    }
    #[test]
    fn colour_and_scene_store() {
        assert_eq!(
            bytes(&p(&["send", "1", "ct", "4000"])),
            vec![
                (0xa3, 250),
                (0xc3, 0),
                (0xc1, 8),
                (3, 0xe7),
                (0xc1, 8),
                (3, 0xe2)
            ]
        );
        assert_eq!(
            bytes(&p(&["write-scene", "1", "2", "128", "ct", "4000"])),
            vec![
                (0xa3, 250),
                (0xc3, 0),
                (0xc1, 8),
                (3, 0xe7),
                (0xa3, 128),
                (3, 0x41),
                (3, 0x41)
            ]
        );
        assert_eq!(
            bytes(&p(&["send", "1", "rgb", "1", "2", "3"])),
            vec![
                (0xa3, 1),
                (0xc3, 2),
                (0xc5, 3),
                (0xc1, 8),
                (3, 0xeb),
                (0xa3, 0x80),
                (0xc1, 8),
                (3, 0xed),
                (0xc1, 8),
                (3, 0xe2)
            ]
        );
        let legacy = plan_with_rgb_mode(
            &["send", "1", "rgb", "1", "2", "3"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "legacy",
        )
        .unwrap();
        assert_eq!(bytes(&legacy)[5], (0xa3, 0x07));
        assert_eq!(
            bytes(&p(&["send", "1", "xy", "0.25", "0.5"]))[0..4],
            [(0xa3, 0), (0xc3, 0x40), (0xc1, 8), (3, 0xe0)]
        );
        let waf = p(&["send", "1", "rgbwaf", "1", "2", "3", "4", "5", "6"]);
        assert!(bytes(&waf).contains(&(3, 0xec)));
        assert_eq!(
            bytes(&p(&["send", "1", "intensity", "mask"])),
            vec![(2, 255)]
        );
    }
    #[test]
    fn rejects_invalid() {
        for args in [
            vec!["send", "64", "level", "1"],
            vec!["send", "1", "scene", "0"],
            vec!["send", "1", "xy", "1.0", "0.3"],
            vec!["send", "1", "xy", "0.7", "0.7"],
            vec!["send", "1", "rgb", "255", "0", "0"],
            vec!["set", "1", "min-level", "mask"],
        ] {
            assert!(plan(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_err());
        }
    }
}
