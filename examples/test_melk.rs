// Safe interactive test for MELK-OA10 power, brightness, and color
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
    println!("Connecting to BlueZ session...");
    let session = Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let addr = Address::new([0xBE, 0x69, 0xDF, 0x01, 0x46, 0x00]);
    println!("Target device: {} (MELK-OA10   45)", addr);

    println!("Ensuring device is present in BlueZ by running discovery...");
    let discover = adapter.discover_devices().await?;
    let mut found = false;
    for _ in 0..10 {
        if let Ok(dev) = adapter.device(addr) {
            if dev.name().await.is_ok() {
                found = true;
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    drop(discover);
    println!("Device discovery check ready (found: {})", found);

    let device = adapter.device(addr)?;
    if !device.is_connected().await.unwrap_or(false) {
        println!("Connecting to {}...", addr);
        match tokio::time::timeout(Duration::from_secs(10), device.connect()).await {
            Ok(Ok(())) => println!("Connected!"),
            Ok(Err(e)) => {
                eprintln!("Connect failed: {}", e);
                return Ok(());
            }
            Err(_) => {
                eprintln!("Connect timed out");
                return Ok(());
            }
        }
    }

    // Wait for services
    for _ in 0..10 {
        if device.is_services_resolved().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(300)).await;
    }

    let mut write_char = None;
    for service in device.services().await? {
        if service.uuid().await? == SERVICE_UUID {
            for charac in service.characteristics().await? {
                if charac.uuid().await? == WRITE_CHAR_UUID {
                    write_char = Some(charac);
                    break;
                }
            }
        }
    }

    let Some(charac) = write_char else {
        eprintln!(
            "Write characteristic {} not found on device!",
            WRITE_CHAR_UUID
        );
        let _ = device.disconnect().await;
        return Ok(());
    };

    println!("Found write characteristic {}!", WRITE_CHAR_UUID);
    let flags = charac.flags().await?;
    println!("Characteristic flags: {:?}", flags);

    let req = CharacteristicWriteRequest {
        op_type: if flags.write_without_response {
            WriteOp::Command
        } else {
            WriteOp::Request
        },
        ..Default::default()
    };

    // Test 1: Purple (R=128, G=0, B=128)
    println!("\n[1/4] Setting Purple (R=128, G=0, B=128)...");
    let purple_pkt = [0x7E, 0x07, 0x05, 0x03, 128, 0, 128, 0x10, 0xEF];
    charac.write_ext(&purple_pkt, &req).await?;
    println!("Sent: {:02X?}", purple_pkt);
    sleep(Duration::from_secs(3)).await;

    // Test 2: Brightness adjustment to 30%
    println!("\n[2/4] Setting Brightness to 30%...");
    let bright_pkt = [0x7E, 0x04, 0x01, 30, 0x01, 0xFF, 0xFF, 0x00, 0xEF];
    charac.write_ext(&bright_pkt, &req).await?;
    println!("Sent: {:02X?}", bright_pkt);
    sleep(Duration::from_secs(3)).await;

    // Test 3: Power Off
    println!("\n[3/4] Turning OFF...");
    let off_pkt = [0x7E, 0x04, 0x04, 0x00, 0x00, 0x00, 0xFF, 0x00, 0xEF];
    charac.write_ext(&off_pkt, &req).await?;
    println!("Sent: {:02X?}", off_pkt);
    sleep(Duration::from_secs(2)).await;

    // Test 4: Power On back to Blue (safe 50% power)
    println!("\n[4/4] Turning ON and restoring Blue...");
    let on_pkt = [0x7E, 0x04, 0x04, 0xF0, 0x00, 0x01, 0xFF, 0x00, 0xEF];
    charac.write_ext(&on_pkt, &req).await?;
    sleep(Duration::from_millis(500)).await;

    let blue_pkt = [0x7E, 0x07, 0x05, 0x03, 0, 0, 128, 0x10, 0xEF];
    charac.write_ext(&blue_pkt, &req).await?;
    println!("Restored Blue safely.");

    sleep(Duration::from_secs(1)).await;
    let _ = device.disconnect().await;
    println!("Cleanly disconnected.");

    Ok(())
}
