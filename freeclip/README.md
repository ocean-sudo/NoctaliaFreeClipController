# FreeClip 2

Noctalia Shell v5 plugin for HUAWEI FreeClip 2. The bar, panel, and shortcut are Luau. A small Rust daemon, `freeclipd`, speaks the vendor SPP channel. Pairing, audio routing, and system volume stay with Noctalia.

The protocol facts are reimplemented from public FreeClip 2 research. This plugin does not ship or run OpenFreebuds. Do not run another SPP client at the same time.

## Plugin

| Field | Value |
| --- | --- |
| ID | `zigzag/freeclip` |
| API | 43 |
| Entries | Bar widget `battery`, panel `panel`, shortcut `latency`, service `daemon` |

## Requirements

- Noctalia v5 with plugin API 43 (tested against v5.4.0).
- BlueZ, with the headset already paired. The daemon opens RFCOMM channel 1 only after BlueZ reports the headset connected. It never calls `Device.Connect`.
- Scrolling the bar widget writes volume through `noctalia msg`. The panel does not contain a volume slider, mute button, or Bluetooth shortcut.
- Rust stable to build the daemon (built with rustc 1.100). The committed tree does not contain the binary.

## Build

From the repository root:

```sh
make build
```

That writes `freeclip/bin/freeclipd` (mode 0755). The directory is gitignored.

```sh
make check   # cargo test and noctalia plugins lint
```

## Install

Path source, from the repository root. Noctalia has to be running before `enable`:

```sh
noctalia msg plugins source add freeclip path "$PWD"
noctalia msg plugins enable zigzag/freeclip
```

Or copy into the user plugin directory without removing other files:

```sh
make install
noctalia msg plugins enable zigzag/freeclip
```

`make install` uses `$XDG_DATA_HOME/noctalia/plugins/freeclip`, or `~/.local/share/noctalia/plugins/freeclip`. Enabling is separate.

Add the bar widget, then put `freeclip` on a bar:

```toml
[widget.freeclip]
type = "zigzag/freeclip:battery"
```

## Usage

- Left click the bar widget toggles this panel next to the widget. Right click asks the daemon to refresh. Scroll changes the system volume by 5% per detent.
- The panel front page shows battery and the low-latency switch. That page hugs its content, up to 560. The settings page, and a dual-device page with several peers, scroll inside that cap. Sound (equalizer, quality, and voice language) and dual-device pairing each open on their own page. Gestures, pause-when-removed, and the model and serial numbers share the settings page opened by the gear immediately left of refresh. Unpair is the destructive button.
- The shortcut tile toggles low latency. Right click opens the panel.
- Low battery notifies once per dip, for the earbuds only. It arms again after the level rises more than 5 points above the threshold. A 0 on one side is ignored while the other side still matches the global level.
- Every SPP frame is appended to `~/.local/state/noctalia-freeclip/packets.log`, or under `XDG_STATE_HOME` when that is set. `FREECLIP_DEBUG=0` turns the log off. The file rotates to `packets.log.1` after 1MB.

```sh
noctalia msg panel-toggle zigzag/freeclip:panel
noctalia msg plugin zigzag/freeclip:daemon all refresh
noctalia msg plugin zigzag/freeclip:daemon all latency on
noctalia msg plugin zigzag/freeclip:daemon all latency off
noctalia msg plugin zigzag/freeclip:daemon all latency toggle
```

## Settings

Plugin settings apply to every entry. Changing them restarts the service, and the new daemon takes the socket from the previous one.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `binary_path` | string | empty | `freeclipd` path. Empty uses `freeclip/bin/freeclipd`, then `PATH`. |
| `device_address` | string | empty | Skip the name match and use this Bluetooth address. |
| `low_battery_notify` | bool | true | Notify when an earbud reaches the threshold. |
| `low_battery_percent` | int | 15 | Threshold from 5 to 50. |

Bar widget settings:

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `battery_style` | select | `min` | `min`, `left`, `right`, or `both`. |
| `show_case` | bool | false | Include the case in `min` and `both`. |
| `hide_when_disconnected` | bool | true | Hide the widget unless the daemon is starting, connecting, ready, or reconnecting. |

## Limits

SPP has not been tested against a headset. Unit tests cover the frame codec and the captured battery, auto-pause, double-tap, and swipe packets. Noise control is expected to time out on this open-ear model and is then hidden. Wearing stays unknown until the headset sends it. The current voice language can stay empty until it is set. The daemon only matches a name containing `FreeClip 2`, not FreeClip 1 or Pro.

## License

MIT. See the repository `LICENSE`.
