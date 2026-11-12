# DaliInspector

Rust logger and control gear scanner for the older Lunatone DALI USB interface (USB VID `17B5`, PID `0020`). The DALI-2 USB serial/LUBA interface is a different device and is not supported here.

```sh
cargo build --release
./target/release/dali-inspector list
./target/release/dali-inspector log --raw
./target/release/dali-inspector scan > gear.json
./target/release/dali-inspector inspect 12 > gear-12.json
```

Use `--device-index N` if `list` shows more than one matching HID interface. Pipe `log` output to `tee` to save a log. The log includes common DALI command names and decodes answers where it can associate an answer with the preceding query.

`scan` queries short addresses 0–63 and prints one JSON object containing every responding control gear. `inspect N` queries one short address. The JSON includes general IEC 62386 control gear query values, status flags, device type(s), levels, fade codes, groups, all 16 scenes, random address, and memory bank 0 identity fields (when readable). A missing query answer appears as `null`. `scenes[].level: null` also means the scene is unprogrammed (raw value `0xFF`). JSON is written to stdout; discovery messages go to stderr. A scan keeps going after an address times out, records it in `errors`, sets `complete: false`, and exits with an error status.

The scan also reports whether any control gear answers a broadcast presence query and whether any gear reports a missing short address. An empty `control_gear` array with `unaddressed_control_gear_present: true` means at least one gear is on the bus but cannot yet be inspected by short address.

The scanner sends standard 16-bit query frames. Reading memory bank 0 also sets the DTR0 and DTR1 registers to select the bank and location; these are transient protocol registers shared by DALI masters. It does not change light levels, addresses, scenes, groups or nonvolatile settings. Device-type-specific queries, DALI-2 control-device instances, and manufacturer-specific memory banks are not yet decoded.

The `decode` command accepts whitespace-separated hex bytes from stdin for offline inspection:

```sh
printf '11 73 00 00 FF 93 FF FF 00\n' | cargo run -- decode --raw
```

Known incoming report layout (64 byte HID report; unused bytes follow the header):

| Offset | Meaning |
| --- | --- |
| 0 | Direction: `11` bus traffic, `12` host transaction |
| 1 | Type: `73` completed 16 bit frame, `74` 24 bit frame, `72` answer, `71` no answer |
| 3–5 | Frame bytes or answer value at offset 5 |
| 6–7 | Device status bytes (meaning unverified) |
| 8 | Host transaction sequence number |

The parser tolerates a leading zero HID report ID. Unknown reports are printed with all bytes. The type `74` is described as a 24 bit bus frame in the open source driver; the exact interpretation of extended and DALI-2 frames should be checked against captures from this particular adapter.

## Protocol references and limits

- Lunatone's [DALI USB datasheet](https://www.lunatone.com/wp-content/uploads/2018/03/24138923_DALI_USB_EN_D0052-1.pdf) identifies the older unit as HID VID `17B5`, PID `0020`.
- The HID report layout is also described by the [daliserver USB implementation](https://github.com/onitake/daliserver/blob/master/lib/usb.c).
- The control gear query codes and memory bank 0 layout were checked against [python-dali's gear commands](https://github.com/sde1000/python-dali/blob/master/dali/gear/general.py) and [memory bank definition](https://github.com/sde1000/python-dali/blob/master/dali/memory/info.py).
- The adapter is initialized when opened. General queries and memory bank 0 fields depend on what the connected control gear supports. Memory reads have automated decoding tests but have not yet been checked against addressed gear on a live bus.

## Troubleshooting

If a build reports that `libc` or `num_traits` is missing in `rlib` format, remove the generated release artifacts and rebuild:

```sh
cargo clean --release
cargo build --release
```

The older DALI USB interface also needs power from the DALI line as well as USB. A USB connection alone can appear in `list` while bus queries receive no answer.
