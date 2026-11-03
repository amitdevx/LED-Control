// Test smooth streaming and hardware effects on MELK-OA10
use bluer::gatt::remote::CharacteristicWriteRequest;
use bluer::gatt::WriteOp;
use bluer::{Address, Session};
use palette::{FromColor, Hsl, Srgb};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

const SERVICE_UUID: Uuid = Uuid::from_u128(0x0000fff0_0000_1000_8000_00805f9b34fb);
const WRITE_CHAR_UUID: Uuid = Uuid::from_u128(0x0000fff3_0000_1000_8000_00805f9b34fb);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = Session::new().await?;
    let adapter = session.default_adapter().await?;
    let addr = Address::new([0xBE, 0x69, 0xDF, 0x01, 0x46, 0x00]);

    println!("Ensuring device is available in BlueZ cache...");
    let discover = adapter.discover_devices().await?;
    let mut found = false;
    for _ in 0..10 {
        if let Ok(dev) = adapter.device(addr) {
            if dev.name().await.is_ok() {
                found = true;
                break;
            }
        }
        sleep(Duration::from_millis(300)).await;
    }
    drop(discover);
    println!("Device discovery ready: {}", found);

    let device = adapter.device(addr)?;
    if !device.is_connected().await.unwrap_or(false) {
        println!("Connecting...");
        device.connect().await?;
    }

    for _ in 0..15 {
        if device.is_services_resolved().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }

    let mut write_char = None;
    for service in device.services().await? {
        if service.uuid().await? == SERVICE_UUID {
            for c in service.characteristics().await? {
                if c.uuid().await? == WRITE_CHAR_UUID {
                    write_char = Some(c);
                    break;
                }
            }
        }
    }

    let charac = write_char.expect("Write char not found");
    let req = CharacteristicWriteRequest {
        op_type: WriteOp::Command, // write without response for fast streaming
        ..Default::default()
    };

    println!("\n=== TEST 1: Smooth 30 Hz Hue Cycle Streaming for 6 seconds ===");
    let start = Instant::now();
    let mut last_send = Instant::now();
    let frame_interval = Duration::from_millis(33); // ~30 fps

    while start.elapsed() < Duration::from_secs(6) {
        let now = Instant::now();
        if now.duration_since(last_send) >= frame_interval {
            last_send = now;
            let elapsed = start.elapsed().as_secs_f64();
            let hue = (elapsed * 60.0 % 360.0) as f32; // 60 deg/sec
            let hsl = Hsl::new(hue, 1.0, 0.5);
            let srgb = Srgb::from_color(hsl);
            let r = (srgb.red * 200.0) as u8;
            let g = (srgb.green * 200.0) as u8;
            let b = (srgb.blue * 200.0) as u8;

            let pkt = [0x7E, 0x00, 0x05, 0x03, r, g, b, 0x00, 0xEF];
            let _ = charac.write_ext(&pkt, &req).await;
        }
        sleep(Duration::from_millis(5)).await;
    }

    println!("\n=== TEST 2: Smooth Cross-fade from Red -> Blue over 3 seconds ===");
    let fade_start = Instant::now();
    let fade_duration = 3.0;
    while fade_start.elapsed().as_secs_f64() < fade_duration {
        let t = (fade_start.elapsed().as_secs_f64() / fade_duration).clamp(0.0, 1.0);
        // Smoothstep interpolation
        let smooth_t = t * t * (3.0 - 2.0 * t);
        let r = (180.0 * (1.0 - smooth_t)) as u8;
        let b = (180.0 * smooth_t) as u8;
        let pkt = [0x7E, 0x00, 0x05, 0x03, r, 0, b, 0x00, 0xEF];
        let _ = charac.write_ext(&pkt, &req).await;
        sleep(Duration::from_millis(30)).await;
    }

    println!("\n=== TEST 3: Hardware Built-in Pattern Test (Fade RGB, ID 144) ===");
    // Hardware effect: 0x7E, 0x05, 0x03, effect_id, 0x06, 0xFF, 0xFF, 0x00, 0xEF
    let hw_pkt = [0x7E, 0x05, 0x03, 144, 0x06, 0xFF, 0xFF, 0x00, 0xEF];
    charac.write_ext(&hw_pkt, &req).await?;
    println!("Sent Hardware Pattern 144 (Fade RGB)");
    sleep(Duration::from_secs(5)).await;

    // Restore to comfortable static Blue
    let blue_pkt = [0x7E, 0x07, 0x05, 0x03, 0, 0, 150, 0x10, 0xEF];
    charac.write_ext(&blue_pkt, &req).await?;
    sleep(Duration::from_millis(500)).await;

    let _ = device.disconnect().await;
    println!("Smoothness test finished cleanly.");
    Ok(())
}
