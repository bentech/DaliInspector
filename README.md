# DaliInspector

Rust logger and control gear scanner for the older Lunatone DALI USB interface (USB VID `17B5`, PID `0020`). The DALI-2 USB serial/LUBA interface is a different device and is not supported here.

## Download a release

Download the executable for your platform from [GitHub Releases](https://github.com/bentech/DaliInspector/releases/latest). Release assets cover macOS Intel (`x86_64`), macOS Apple Silicon (`aarch64`), Windows x64 and ARM64, and Linux x64 and ARM64, with a `SHA256SUMS` file. On macOS and Linux, make the downloaded file executable with `chmod +x dali-inspector-*`. Linux builds use the system `libudev` runtime library.

The [Build executables workflow](https://github.com/bentech/DaliInspector/actions/workflows/build.yml) runs on pushes to `master`, version tags beginning with `v`, pull requests, and manual dispatch. It compiles and runs the test suite on each platform; it does not need a USB adapter to build. Successful version tag builds publish the six executables as a GitHub Release. Other successful runs keep their downloadable workflow artifacts.

### Install with a package manager

Cargo can install directly from the private Git repository if you have access and the Rust toolchain:

```sh
cargo install --git ssh://git@github.com/bentech/DaliInspector.git --tag v1.0.0 --locked
```

On macOS, Homebrew can install from the formula in this repository. First add it as a private tap using your GitHub SSH access:

```sh
brew tap bentech/dali-inspector git@github.com:bentech/DaliInspector.git
brew install bentech/dali-inspector/dali-inspector
```

The formula builds from the fixed v1.0.0 source tag. This repository and its tap remain private, so installers need GitHub access.

## Build locally

```sh
cargo build --release
./target/release/dali-inspector list
./target/release/dali-inspector log --raw
./target/release/dali-inspector follow --raw
./target/release/dali-inspector scan > gear.json
./target/release/dali-inspector inspect 12 > gear-12.json
./target/release/dali-inspector send 12 level 128
./target/release/dali-inspector send 12 scene 2
./target/release/dali-inspector send 12 ct 4000
./target/release/dali-inspector send 12 xy 0.35 0.35
./target/release/dali-inspector send 12 rgb 254 0 0
./target/release/dali-inspector send 12 rgbwaf 254 0 0 0 0 0
```

Use `follow` (or `--follow`) to watch DALI frames sent by another controller on the same powered DALI bus. This uses the passive `log` mode; add `--raw` to include frame bytes. Use `--device-index N` if `list` shows more than one matching HID interface. Pipe the output to `tee dali-traffic.log` to save it. The log includes common DALI command names and decodes answers where it can associate an answer with the preceding query.

`scan` queries short addresses 0–63 and prints one JSON object containing every responding control gear. `inspect N` queries one short address. The JSON includes general IEC 62386 control gear query values, status flags, device type(s), levels, fade codes, groups, all 16 scenes, random address, and memory bank 0 identity fields (when readable). The two DALI group bytes are combined into one `groups` array of group numbers 0–15 (for example, masks `4` and `0` produce `[2]`). A missing query answer appears as `null`. `scenes[].level: null` also means the scene is unprogrammed (raw value `0xFF`). JSON is written to stdout; discovery messages go to stderr. A scan keeps going after an address times out, records it in `errors`, sets `complete: false`, and exits with an error status.

The scan also reports whether any control gear answers a broadcast presence query and whether any gear reports a missing short address. An empty `control_gear` array with `unaddressed_control_gear_present: true` means at least one gear is on the bus but cannot yet be inspected by short address.

## Sending to control gear

`send` addresses one control gear by short address (0–63). `level` or `intensity` sends direct arc power (0–254); `mask` is also accepted and tells the gear to leave its level unchanged. Scenes shown at the CLI are **1–16**, one higher than the DALI wire scene number. `send ... scene N` recalls the scene. `ct` accepts Kelvin, `xy` accepts CIE x/y coordinates between 0 and 1, and RGB/RGBWAF channels accept DALI dim levels 0–254. Colour commands require compatible DT8 gear; `led-curve` requires DT6 gear.

```sh
dali-inspector set 12 fade-time 7
dali-inspector set 12 extended-fade-time 16
dali-inspector set 12 fade-rate 7
dali-inspector set 12 min-level 5
dali-inspector set 12 max-level 254
dali-inspector set 12 power-on-level mask
dali-inspector set 12 system-failure-level 128
dali-inspector set 12 led-curve linear
dali-inspector write-scene 12 2 128 ct 4000
dali-inspector write-scene 12 3 180 rgb 254 0 0
dali-inspector write-scene 12 4 mask
```

The fade time and fade rate parameters are DALI codes 0–15. Fade time code 0 selects extended fade time; code 7 is about 5.66 seconds. Fade rate code 7 is about 44.7 steps/second. Extended fade time is a DALI code 0–79 (`multiplier × 16 + base`): multiplier 0 is fastest, 1 is 100 ms, 2 is 1 s, 3 is 10 s and 4 is 1 min; base 0–15 represents 1–16. Thus code 16 is 100 ms. For the extended time to take effect, also set fade time to code 0. `power-on-level` and `system-failure-level` accept `mask` (255). `led-curve exponential` selects the DT6 standard logarithmic curve; `linear` selects the linear curve.

`write-scene` stores the given intensity and, when specified, the temporary DT8 colour setting in one scene. The colour setup is sent first, then DTR0 is loaded with the scene intensity and the scene store command is sent twice. This changes persistent gear settings. `send ... ct|xy|rgb|rgbwaf` activates the colour immediately. Use `send ... level` separately for intensity.

RGB and RGBWAF default to current DT8 normalised colour control (`0x80`). Add `--rgb-mode legacy` for the older DT8 linked-channel control values, or `--rgb-mode extended` for current extended colour control (`0xC0`). Use `--dry-run` with any `send`, `set` or `write-scene` command to print the exact DALI address/data bytes without opening USB. These operations use broadcast DTR registers transiently, so another master on the bus could interfere with a multi-frame sequence. The CLI reports frame transmission; use `inspect` or `log` to check how a particular gear reacted.

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
- DT8 colour commands and their DTR order follow [python-dali's colour commands](https://github.com/sde1000/python-dali/blob/master/dali/gear/colour.py) and [colour sequence](https://github.com/sde1000/python-dali/blob/master/dali/gear/sequences.py); DT6 curve selection follows [its LED commands](https://github.com/sde1000/python-dali/blob/master/dali/gear/led.py).
- The current DT8 RGBWAF control values are shown in [Renesas's DALI master manual](https://www.renesas.com/en/document/mat/dali-master-controller-gui-users-manual).
- The adapter is initialized when opened. General queries and memory bank 0 fields depend on what the connected control gear supports. Memory reads have automated decoding tests but have not yet been checked against addressed gear on a live bus.
- Sending a masked level was checked with a connected adapter. Scene, colour and configuration writes have frame tests, but have not yet been checked against addressed gear on a live bus.

## Troubleshooting

If a build reports that `libc` or `num_traits` is missing in `rlib` format, remove the generated release artifacts and rebuild:

```sh
cargo clean --release
cargo build --release
```

The older DALI USB interface also needs power from the DALI line as well as USB. A USB connection alone can appear in `list` while bus queries receive no answer.
