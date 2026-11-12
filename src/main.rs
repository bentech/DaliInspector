use chrono::Local;
use dali_inspector::control::{self, Frame};
use dali_inspector::scan::QueryBus;
use dali_inspector::{
    command_name, decode, describe, describe_answer, hex, is_query, parse_hex_line, query_payload,
    scan, Report, PRODUCT_ID, VENDOR_ID,
};
use hidapi::{HidApi, HidDevice};
use serde_json::json;
use std::env;
use std::error::Error;
use std::io::{self, BufRead};
use std::time::{Duration, Instant};

fn usage() -> &'static str {
    "Usage:\n  dali-inspector list\n  dali-inspector log [--device-index N] [--raw]\n  dali-inspector scan [--device-index N]\n  dali-inspector inspect SHORT_ADDRESS [--device-index N]\n  dali-inspector send SHORT level 0..254|mask | scene 1..16 | ct KELVIN | xy X Y | rgb R G B | rgbwaf R G B W A F\n  dali-inspector set SHORT fade-time CODE | extended-fade-time CODE | fade-rate CODE | min-level LEVEL | max-level LEVEL | power-on-level LEVEL|mask | system-failure-level LEVEL|mask | led-curve linear|exponential\n  dali-inspector write-scene SHORT SCENE LEVEL|mask [ct KELVIN | xy X Y | rgb R G B | rgbwaf R G B W A F]\n  dali-inspector decode [--raw] < reports.hex\n\nFor send, set and write-scene: append --dry-run to print frames without USB, --device-index N, or --rgb-mode normalised|extended|legacy. Scenes shown are 1..16. Ctrl-C stops logging."
}

struct HidBus {
    handle: HidDevice,
    next_sequence: u8,
}

impl HidBus {
    fn send_frame(&mut self, frame: &Frame) -> Result<(), String> {
        let sequence = self.next_sequence;
        self.next_sequence = if sequence == 255 { 1 } else { sequence + 1 };
        let mut report = [0u8; 65];
        report[1..].copy_from_slice(&query_payload(sequence, frame.address, frame.data));
        let written = self.handle.write(&report).map_err(|e| e.to_string())?;
        if written != report.len() {
            return Err(format!(
                "short HID write: {written} of {} bytes",
                report.len()
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut incoming = [0u8; 65];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("timeout waiting for USB sequence {sequence}"));
            }
            let count = self
                .handle
                .read_timeout(&mut incoming, left.as_millis().min(200).max(1) as i32)
                .map_err(|e| e.to_string())?;
            if count == 0 {
                continue;
            }
            match decode(&incoming[..count]) {
                Ok(Report::HostComplete { sequence: got, .. }) if got == sequence => return Ok(()),
                Ok(Report::NoAnswer { sequence: got, .. }) if got == sequence => return Ok(()),
                Ok(Report::Answer { sequence: got, .. }) if got == sequence => {
                    return Err("unexpected DALI answer to a send frame".into())
                }
                _ => {}
            }
        }
    }
}

impl scan::QueryBus for HidBus {
    fn transact(&mut self, address: u8, command: u8) -> Result<Option<u8>, String> {
        let sequence = self.next_sequence;
        self.next_sequence = if sequence == 255 { 1 } else { sequence + 1 };
        let mut report = [0u8; 65]; // HID report ID zero, then 64 payload bytes.
        report[1..].copy_from_slice(&query_payload(sequence, address, command));
        let written = self.handle.write(&report).map_err(|e| e.to_string())?;
        if written != report.len() {
            return Err(format!(
                "short HID write: {written} of {} bytes",
                report.len()
            ));
        }
        let timeout_ms = if command == 0x91 { 500 } else { 1500 };
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut incoming = [0u8; 65];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("timeout waiting for USB sequence {sequence}"));
            }
            let wait_ms = left.as_millis().min(200).max(1) as i32;
            let count = self
                .handle
                .read_timeout(&mut incoming, wait_ms)
                .map_err(|e| e.to_string())?;
            if count == 0 {
                continue;
            }
            match decode(&incoming[..count]) {
                Ok(dali_inspector::Report::Answer {
                    value,
                    sequence: got,
                    ..
                }) if got == sequence => return Ok(Some(value)),
                Ok(dali_inspector::Report::NoAnswer { sequence: got, .. }) if got == sequence => {
                    return Ok(None)
                }
                _ => {} // Bus traffic, completion, or response to a different sequence.
            }
        }
    }
}

fn open_hid_bus(api: &HidApi, device_index: usize) -> Result<HidBus, Box<dyn Error>> {
    let device = api
        .device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
        .nth(device_index)
        .ok_or_else(|| {
            format!("DALI USB HID device index {device_index} not found; run `dali-inspector list`")
        })?;
    let handle = device.open_device(api)?;
    initialize(&handle)?;
    Ok(HidBus {
        handle,
        next_sequence: 1,
    })
}

fn initialize(handle: &HidDevice) -> Result<(), Box<dyn Error>> {
    let mut output = [0u8; 65];
    output[1] = 0x01;
    for request in 0..=1 {
        output[2] = request;
        let written = handle.write(&output)?;
        if written != output.len() {
            return Err(format!("short initialization write: {written} bytes").into());
        }
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut input = [0u8; 65];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(
                    format!("adapter did not answer initialization request {request}").into(),
                );
            }
            let received =
                handle.read_timeout(&mut input, left.as_millis().min(200).max(1) as i32)?;
            if received >= 5 && input[0] == 0x01 {
                break;
            }
        }
    }
    Ok(())
}

fn display(bytes: &[u8], raw: bool, last_query: &mut Option<(u8, u8)>) {
    let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f%:z");
    let description = match decode(bytes) {
        Ok(report) => {
            let mut description = describe(report);
            match report {
                Report::Forward16 { address, data, .. } => {
                    *last_query = (address & 1 == 1 && is_query(data)).then_some((address, data));
                }
                Report::BusAnswer { value, .. } => {
                    if let Some((address, command)) = last_query.take() {
                        description.push_str(&format!(
                            " response_to=\"{:02X} {}\" {}",
                            address,
                            command_name(address, command),
                            describe_answer(command, value)
                        ));
                    }
                }
                Report::BusNoAnswer { .. } => {
                    *last_query = None;
                }
                _ => {}
            }
            description
        }
        Err(error) => format!("invalid: {error}"),
    };
    if raw || description.starts_with("unknown") || description.starts_with("invalid") {
        println!("{time} {description} | {}", hex(bytes));
    } else {
        println!("{time} {description}");
    }
}

fn list(api: &HidApi) {
    let devices: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
        .collect();
    if devices.is_empty() {
        println!("No DALI USB HID device found (VID {VENDOR_ID:04X}, PID {PRODUCT_ID:04X}).");
        return;
    }
    for (index, device) in devices.iter().enumerate() {
        println!(
            "{index}: {} | serial={} | interface={} | path={}",
            device.product_string().unwrap_or("DALI USB"),
            device.serial_number().unwrap_or("unknown"),
            device.interface_number(),
            device.path().to_string_lossy()
        );
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let all_args: Vec<String> = env::args().skip(1).collect();
    if matches!(
        all_args.first().map(String::as_str),
        Some("send" | "set" | "write-scene")
    ) {
        return run_control(&all_args);
    }
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let mut raw = false;
    let mut device_index = 0usize;
    let mut short_address = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" => raw = true,
            "--device-index" => {
                device_index = args
                    .next()
                    .ok_or("--device-index needs a number")?
                    .parse()?
            }
            _ if command == "inspect" && short_address.is_none() && !arg.starts_with('-') => {
                let number: u8 = arg.parse()?;
                if number > 63 {
                    return Err("short address must be 0..63".into());
                }
                short_address = Some(number);
            }
            _ => return Err(format!("unknown option {arg}\n{}", usage()).into()),
        }
    }
    match command.as_str() {
        "help" | "--help" | "-h" => println!("{}", usage()),
        "decode" => {
            let mut last_query = None;
            for (line_number, line) in io::stdin().lock().lines().enumerate() {
                let line = line?;
                let bytes =
                    parse_hex_line(&line).map_err(|e| format!("line {}: {e}", line_number + 1))?;
                if !bytes.is_empty() {
                    display(&bytes, raw, &mut last_query);
                }
            }
        }
        "list" => {
            let api = HidApi::new()?;
            list(&api);
        }
        "scan" => {
            let api = HidApi::new()?;
            let mut bus = open_hid_bus(&api, device_index)?;
            let mut devices = Vec::new();
            let mut errors = Vec::new();
            for short in 0..64 {
                match scan::inspect(&mut bus, short) {
                    Ok(Some(device)) => {
                        eprintln!("Found control gear at short address {short}");
                        devices.push(device);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("Short address {short}: {error}");
                        errors.push(json!({"short_address": short, "error": error}));
                    }
                }
            }
            let broadcast_control_gear_present = match bus.transact(0xff, 0x91) {
                Ok(answer) => Some(answer.is_some()),
                Err(error) => {
                    errors.push(json!({"query": "broadcast_control_gear_present", "error": error}));
                    None
                }
            };
            let unaddressed_control_gear_present = match bus.transact(0xff, 0x96) {
                Ok(answer) => Some(answer.is_some()),
                Err(error) => {
                    errors
                        .push(json!({"query": "unaddressed_control_gear_present", "error": error}));
                    None
                }
            };
            let complete = errors.is_empty();
            if devices.is_empty() && unaddressed_control_gear_present == Some(true) {
                eprintln!("Control gear is present, but at least one device has no short address");
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "interface": "DALI USB HID 17B5:0020",
                    "scanned_short_addresses": 64,
                    "control_gear": devices,
                    "broadcast_control_gear_present": broadcast_control_gear_present,
                    "unaddressed_control_gear_present": unaddressed_control_gear_present,
                    "complete": complete,
                    "errors": errors
                }))?
            );
            if !complete {
                return Err("scan had query errors; see the JSON errors array".into());
            }
        }
        "inspect" => {
            let short = short_address.ok_or("inspect needs a short address (0..63)")?;
            let api = HidApi::new()?;
            let mut bus = open_hid_bus(&api, device_index)?;
            let result = scan::inspect(&mut bus, short)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "interface": "DALI USB HID 17B5:0020",
                    "control_gear": result,
                    "short_address": short,
                    "present": result.is_some()
                }))?
            );
        }
        "log" => {
            let api = HidApi::new()?;
            let bus = open_hid_bus(&api, device_index)?;
            eprintln!("Listening to DALI USB HID (VID {VENDOR_ID:04X}, PID {PRODUCT_ID:04X}); Ctrl-C to stop");
            let mut buffer = [0u8; 65];
            let mut last_query = None;
            loop {
                let count = bus.handle.read_timeout(&mut buffer, 1000)?;
                if count > 0 {
                    display(&buffer[..count], raw, &mut last_query);
                }
            }
        }
        _ => return Err(format!("unknown command {command}\n{}", usage()).into()),
    }
    Ok(())
}

fn run_control(args: &[String]) -> Result<(), Box<dyn Error>> {
    let mut positional = Vec::new();
    let mut dry_run = false;
    let mut device_index = 0usize;
    let mut rgb_mode = "normalised";
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry_run = true,
            "--device-index" => {
                i += 1;
                device_index = args
                    .get(i)
                    .ok_or("--device-index needs a number")?
                    .parse()?;
            }
            "--rgb-mode" => {
                i += 1;
                rgb_mode = args.get(i).ok_or("--rgb-mode needs a value")?;
            }
            _ => positional.push(args[i].clone()),
        }
        i += 1;
    }
    if !matches!(rgb_mode, "normalised" | "extended" | "legacy") {
        return Err("--rgb-mode must be normalised, extended or legacy".into());
    }
    let frames = control::plan_with_rgb_mode(&positional, rgb_mode)?;
    if dry_run {
        for (index, frame) in frames.iter().enumerate() {
            println!(
                "{:02} {:02X} {:02X} {}",
                index + 1,
                frame.address,
                frame.data,
                frame.label
            );
        }
        return Ok(());
    }
    let api = HidApi::new()?;
    let mut bus = open_hid_bus(&api, device_index)?;
    for (index, frame) in frames.iter().enumerate() {
        bus.send_frame(frame)
            .map_err(|error| format!("frame {} ({}): {error}", index + 1, frame.label))?;
    }
    println!(
        "Sent {} DALI frames to short address {}",
        frames.len(),
        positional[1]
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
