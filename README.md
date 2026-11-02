# One94Store Smart LED Strip Controller (Linux)

Native high-performance Linux desktop application for controlling the One94Store 5-meter Smart RGB LED Strip Light with BLE, remote effects, and music synchronization.

## Target Hardware Specifications

- Product: One94Store 5M Smart LED Strip Light
- Model: SR-SMD-SM
- ASIN: B0FTQ5Q7BZ
- Protocol: LEDDMX 9-byte frame
- GATT Service UUID: `0000ffe0-0000-1000-8000-00805f9b34fb` (0xFFE0)
- GATT Characteristic UUID: `0000ffe1-0000-1000-8000-00805f9b34fb` (0xFFE1)
- Default Channel Order: RBG (Red, Blue, Green; green and blue swapped on SR-SMD-SM)

## Features

- Native Modern Stack: Written in pure Rust with GTK4, Cairo GPU-rendered canvas, and BlueZ D-Bus bindings (`bluer`).
- Real-time Strip Simulation: Interactive, animated horizontal LED strip visualizing lighting state, diffusion glow, and specular highlights at 30 fps.
- 16 Million Colors: Interactive RGB channel sliders, hex entry, and quick color presets.
- Dynamic Effects Engine: Smooth hue cycling, breathing, strobe, warm fade, cool fade, rainbow jump, candle flicker, and cross-fade.
- 210 Hardware Patterns: Direct selection and sending of the controller's onboard microchip pattern routines.
- Music Sync: Audio-reactive rhythm synchronization featuring Beat Pulse, Beat Jump, Energy Wave, and Rhythm Flash with live PulseAudio/PipeWire capture and BPM tempo generator.
- BLE Discovery and Pairing: Built-in Bluetooth Low Energy scanner for `LEDDMX-*` devices with RSSI metrics.
- Channel Calibration: Live switching across RGB, RBG, GRB, GBR, BRG, and BGR configurations.

## Building and Running

### Prerequisites

Ensure GTK4, BlueZ, and standard build tools are present:

```bash
sudo apt-get install -y pkg-config libgtk-4-dev libdbus-1-dev libbluetooth-dev
```

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

### Run Unit Tests

```bash
cargo test
```


## Verified implementation boundary

The application now treats these as separate states:

- discovered: a matching LEDDMX* advertisement was observed;
- connecting: BlueZ is attempting the link;
- discovering services: the link exists and GATT is being enumerated;
- ready: the expected FFE0 service and writable FFE1 characteristic were found;
- sending: one serialized command is being written;
- command sent: BlueZ accepted the write request;
- command failed: the write was rejected or the controller was not GATT-ready.

A successful write means BlueZ accepted the ATT operation. It does not prove that the controller displayed the requested colour.

Static colour frames preserve the requested logical RGB values and apply the configured channel order in the protocol module. The optional power limiter affects software effects only; it does not mutate static-colour wire packets.

## Linux hardware procedure

1. Close LED LAMP, the phone app, nRF Connect, and any other client. The controller normally accepts one BLE connection.
2. Confirm the adapter and BlueZ service without root-running this GUI:

   bluetoothctl list
   systemctl is-active bluetooth
   rfkill list bluetooth

3. Start with verbose logs:

   RUST_LOG=led_controller=debug,bluer=info cargo run --release

4. Press Scan for Devices. Select only a device whose advertised name starts with LEDDMX, LED DMX, or LED-DMX. The scan row tooltip contains captured advertisement counts and TX power when BlueZ exposes them.
5. Press Connect. Wait for Ready; discovery alone is not a connection.
6. Use the Settings calibration buttons in this order: pure red, pure green, pure blue. Confirm the physical strip after each write. If a colour is swapped, select another channel order and repeat. The software must not mark an order verified without that observation.
7. Test power, brightness at 0%, 50%, and 100%, then a known pattern index. Pattern names and indices are firmware-dependent; the UI catalog is not proof that every index exists on the controller.
8. Turn the strip off, move it out of range, or stop the adapter to verify the reconnect status. Re-enable Bluetooth and confirm the bounded reconnect attempts and the final error state.
9. If the controller is not found, close all phone clients first, check adapter power/rfkill/BlueZ, and confirm the advertised name and GATT table in the logs.

## Diagnostics

The BLE worker logs adapter detection, target advertisements, service UUIDs, characteristic flags, descriptors, write mode, exact frame bytes, write failures, disconnects, and reconnect attempts. Capture a run with:

   RUST_LOG=led_controller=debug,bluer=debug cargo run --release 2>&1 | tee led-controller-diagnostics.log

The current repository has no physical controller capture from this machine. Actual discovery, GATT compatibility, packet acceptance, visible output, and reconnect behaviour remain hardware tests.
