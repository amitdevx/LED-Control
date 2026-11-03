// Test verified hardware patterns on MELK-OA10
use bluer::gatt::remote::CharacteristicWriteRequest;
use bluer::gatt::WriteOp;
use bluer::{Address, Session};
use std::time::Duration;
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
        op_type: WriteOp::Command,
        ..Default::default()
    };

    println!("\nTesting Pattern 16: Rainbow Cycle (4 seconds)...");
    let pkt = [0x7E, 0x05, 0x03, 16, 0x06, 0xFF, 0xFF, 0x00, 0xEF];
    charac.write_ext(&pkt, &req).await?;
    sleep(Duration::from_secs(4)).await;

    println!("\nTesting Pattern 48: Breathing (4 seconds)...");
    let pkt = [0x7E, 0x05, 0x03, 48, 0x06, 0xFF, 0xFF, 0x00, 0xEF];
    charac.write_ext(&pkt, &req).await?;
    sleep(Duration::from_secs(4)).await;

    println!("\nTesting Pattern 138: 7-Color Crossfade (4 seconds)...");
    let pkt = [0x7E, 0x05, 0x03, 138, 0x06, 0xFF, 0xFF, 0x00, 0xEF];
    charac.write_ext(&pkt, &req).await?;
    sleep(Duration::from_secs(4)).await;

    // Restore to comfortable static Blue
    let blue_pkt = [0x7E, 0x00, 0x05, 0x03, 0, 0, 150, 0x00, 0xEF];
    charac.write_ext(&blue_pkt, &req).await?;
    sleep(Duration::from_millis(500)).await;

    let _ = device.disconnect().await;
    println!("Pattern verification complete.");
    Ok(())
}
