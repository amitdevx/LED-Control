// Scratch probe to safely inspect GATT services and characteristics of the connected device
use bluer::{AdapterEvent, Address, Session};
use futures::StreamExt;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    println!("Connecting to BlueZ session...");
    let session = Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    println!("Adapter powered on: {}", adapter.name());

    println!("Starting discovery for 8 seconds...");
    let mut discover = adapter.discover_devices().await?;
    let mut found_target: Option<(Address, String)> = None;

    let scan_timeout = sleep(Duration::from_secs(8));
    tokio::pin!(scan_timeout);

    loop {
        tokio::select! {
            _ = &mut scan_timeout => {
                println!("Scan window completed.");
                break;
            }
            Some(event) = discover.next() => {
                if let AdapterEvent::DeviceAdded(addr) = event {
                    if let Ok(dev) = adapter.device(addr) {
                        let name = dev.name().await.ok().flatten().unwrap_or_else(|| "Unknown".to_string());
                        let rssi = dev.rssi().await.ok().flatten();
                        println!("Discovered device: {} ({:?}) - RSSI: {:?}", addr, name, rssi);

                        let upper = name.to_ascii_uppercase();
                        if upper.contains("MELK") || upper.contains("ELK") || upper.contains("LED") || addr == Address::new([0xBE, 0x69, 0xDF, 0x01, 0x46, 0x00]) {
                            println!("=> Found candidate target: {} ({})", addr, name);
                            found_target = Some((addr, name));
                        }
                    }
                }
            }
        }
    }

    drop(discover);

    let target_addr = if let Some((addr, name)) = found_target {
        println!("Proceeding with candidate target: {} ({})", addr, name);
        addr
    } else {
        println!("Checking direct address BE:69:DF:01:46:00...");
        Address::new([0xBE, 0x69, 0xDF, 0x01, 0x46, 0x00])
    };

    println!("Inspecting device: {}", target_addr);
    let device = adapter.device(target_addr)?;

    println!("Attempting safe connection to {}...", target_addr);
    match tokio::time::timeout(Duration::from_secs(10), device.connect()).await {
        Ok(Ok(())) => println!("Connected successfully!"),
        Ok(Err(e)) => {
            eprintln!("Failed to connect: {}", e);
            return Ok(());
        }
        Err(_) => {
            eprintln!("Connection timed out after 10s");
            return Ok(());
        }
    }

    // Give BlueZ time to resolve services
    println!("Resolving GATT services...");
    let mut services_resolved = false;
    for _ in 0..10 {
        if device.is_services_resolved().await.unwrap_or(false) {
            services_resolved = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    if !services_resolved {
        println!("Services not marked resolved yet, proceeding to inspect services directly...");
    }

    let services = device.services().await?;
    println!("Found {} GATT service(s):", services.len());

    for service in services {
        let s_uuid = service.uuid().await?;
        println!("\nService UUID: {}", s_uuid);

        let characteristics = service.characteristics().await?;
        for charac in characteristics {
            let c_uuid = charac.uuid().await?;
            let flags = charac.flags().await?;
            println!("  Characteristic UUID: {} | Flags: {:?}", c_uuid, flags);

            // If read flag is present, read safely
            if flags.read {
                match charac.read().await {
                    Ok(val) => {
                        let text = String::from_utf8(val.clone())
                            .unwrap_or_else(|_| "raw bytes".to_string());
                        println!("    Read value: {:02x?} (utf8: {:?})", val, text);
                    }
                    Err(e) => {
                        println!("    Read failed: {}", e);
                    }
                }
            }
        }
    }

    println!("\nSafe inspection complete. Disconnecting from device...");
    let _ = device.disconnect().await;
    println!("Disconnected cleanly.");

    Ok(())
}
