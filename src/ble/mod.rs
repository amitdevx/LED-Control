// ponytail: BLE worker managing BlueZ communication, cancellable scanning,
// connection lifecycle, bounded reconnects, and serialized GATT writes.

mod protocol;

pub use protocol::{ColorOrder, Command, Protocol};

use bluer::gatt::remote::{Characteristic, CharacteristicWriteRequest};
use bluer::gatt::WriteOp;
use bluer::{
    Adapter, AdapterEvent, Address, Device, DeviceEvent, DiscoveryFilter, DiscoveryTransport,
    Session,
};
use futures::StreamExt;
use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::pin::Pin;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

const SERVICE_UUID: Uuid = Uuid::from_u128(0x0000ffe0_0000_1000_8000_00805f9b34fb);
const CHAR_UUID: Uuid = Uuid::from_u128(0x0000ffe1_0000_1000_8000_00805f9b34fb);
const DEVICE_NAME_PREFIXES: &[&str] = &["LEDDMX", "LED DMX", "LED-DMX"];
const MAX_RECONNECT_ATTEMPTS: u8 = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SCAN_DURATION: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Scanning,
    DeviceFound(String),
    Connecting(String),
    DiscoveringServices(String),
    Ready(String),
    SendingCommand(String),
    Reconnecting(String, u8),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub address: Address,
    pub name: String,
    pub rssi: Option<i16>,
    pub tx_power: Option<i16>,
    pub service_uuids: Vec<Uuid>,
    pub manufacturer_data: Vec<(u16, Vec<u8>)>,
    pub service_data: Vec<(Uuid, Vec<u8>)>,
    pub advertising_data: Vec<(u8, Vec<u8>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    WithResponse,
    WithoutResponse,
}

#[derive(Debug)]
pub enum BleMessage {
    Scan,
    Connect(Address),
    Disconnect,
    SendCommand(Command),
    SetColorOrder(ColorOrder),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum BleEvent {
    StateChanged(ConnectionState),
    DeviceFound(DiscoveredDevice),
    ScanComplete,
    CommandSent,
    CommandFailed(String),
}

#[derive(Debug)]
enum WorkerEvent {
    ConnectionLost(u64),
    Reconnect(u64, Address, u8),
}

#[derive(Clone)]
pub struct BleHandle {
    tx: mpsc::Sender<BleMessage>,
}

impl BleHandle {
    pub fn try_send(&self, message: BleMessage) {
        if let Err(error) = self.tx.try_send(message) {
            warn!("BLE worker queue rejected message: {error}");
        }
    }

    pub fn sender(&self) -> mpsc::Sender<BleMessage> {
        self.tx.clone()
    }
}

struct ActiveScan {
    stream: Pin<Box<dyn futures::Stream<Item = AdapterEvent> + Send>>,
    deadline: tokio::time::Instant,
}

pub struct BleWorker {
    rx: mpsc::Receiver<BleMessage>,
    internal_rx: mpsc::Receiver<WorkerEvent>,
    internal_tx: mpsc::Sender<WorkerEvent>,
    event_tx: Sender<BleEvent>,
    session: Option<Session>,
    adapter: Option<Adapter>,
    device: Option<Device>,
    characteristic: Option<Characteristic>,
    write_mode: Option<WriteMode>,
    active_scan: Option<ActiveScan>,
    monitor_task: Option<JoinHandle<()>>,
    reconnect_task: Option<JoinHandle<()>>,
    reconnect_address: Option<Address>,
    reconnect_attempt: u8,
    connection_generation: u64,
    manual_disconnect: bool,
    protocol: Protocol,
}

impl BleWorker {
    pub fn new(event_tx: Sender<BleEvent>) -> (BleHandle, Self) {
        let (tx, rx) = mpsc::channel(128);
        let (internal_tx, internal_rx) = mpsc::channel(16);
        let handle = BleHandle { tx };
        let worker = Self {
            rx,
            internal_rx,
            internal_tx,
            event_tx,
            session: None,
            adapter: None,
            device: None,
            characteristic: None,
            write_mode: None,
            active_scan: None,
            monitor_task: None,
            reconnect_task: None,
            reconnect_address: None,
            reconnect_attempt: 0,
            connection_generation: 0,
            manual_disconnect: false,
            protocol: Protocol::new(ColorOrder::default()),
        };
        (handle, worker)
    }

    pub async fn run(mut self) {
        info!("BLE worker started");
        self.initialize_bluez().await;

        loop {
            let has_active_scan = self.active_scan.is_some();

            tokio::select! {
                message = self.rx.recv() => {
                    let Some(message) = message else { break };
                    if !self.handle_message(message).await {
                        break;
                    }
                }
                event = self.internal_rx.recv() => {
                    if let Some(event) = event {
                        self.handle_worker_event(event).await;
                    }
                }
                // Non-blocking cancellable scan branch
                scan_event = async {
                    if let Some(scan) = self.active_scan.as_mut() {
                        tokio::select! {
                            _ = tokio::time::sleep_until(scan.deadline) => {
                                ScanResult::Deadline
                            }
                            item = scan.stream.next() => {
                                match item {
                                    Some(ev) => ScanResult::Item(ev),
                                    None => ScanResult::Ended,
                                }
                            }
                        }
                    } else {
                        std::future::pending::<ScanResult>().await
                    }
                }, if has_active_scan => {
                    match scan_event {
                        ScanResult::Item(AdapterEvent::DeviceAdded(address)) => {
                            if let Some(adapter) = self.adapter.clone() {
                                self.inspect_candidate(&adapter, address).await;
                            }
                        }
                        ScanResult::Item(_) => {}
                        ScanResult::Deadline | ScanResult::Ended => {
                            self.stop_scan(true).await;
                        }
                    }
                }
            }
        }

        self.stop_scan(false).await;
        self.disconnect_current().await;
        info!("BLE worker stopped");
    }

    async fn initialize_bluez(&mut self) {
        match Session::new().await {
            Ok(session) => {
                match session.default_adapter().await {
                    Ok(adapter) => {
                        info!("Bluetooth adapter detected: {}", adapter.name());
                        self.adapter = Some(adapter);
                    }
                    Err(error) => {
                        let message = format!(
                            "No Bluetooth adapter available: {error}. Check system service and hardware."
                        );
                        error!("{message}");
                        self.emit_state(ConnectionState::Error(message));
                    }
                }
                self.session = Some(session);
            }
            Err(error) => {
                let message = format!("BlueZ session error: {error}. Check D-Bus and bluetoothd.");
                error!("{message}");
                self.emit_state(ConnectionState::Error(message));
            }
        }
    }

    async fn handle_message(&mut self, message: BleMessage) -> bool {
        match message {
            BleMessage::Scan => {
                self.start_scan().await;
            }
            BleMessage::Connect(address) => {
                self.stop_scan(false).await;
                self.manual_disconnect = false;
                self.reconnect_attempt = 0;
                self.reconnect_address = Some(address);
                self.handle_connect(address, false).await;
            }
            BleMessage::Disconnect => {
                self.stop_scan(false).await;
                self.manual_disconnect = true;
                self.cancel_reconnect();
                self.disconnect_current().await;
                self.emit_state(ConnectionState::Disconnected);
            }
            BleMessage::SendCommand(command) => {
                self.handle_command(command).await;
            }
            BleMessage::SetColorOrder(order) => {
                self.protocol.set_color_order(order);
                info!("Protocol color order set to {order:?}");
            }
            BleMessage::Shutdown => {
                self.stop_scan(false).await;
                self.manual_disconnect = true;
                self.cancel_reconnect();
                return false;
            }
        }
        true
    }

    async fn handle_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::ConnectionLost(generation) => {
                if generation != self.connection_generation || self.manual_disconnect {
                    debug!(
                        "Ignoring stale ConnectionLost generation={generation} current={}",
                        self.connection_generation
                    );
                    return;
                }
                self.disconnect_current().await;
                self.emit_state(ConnectionState::Disconnected);
                self.schedule_reconnect();
            }
            WorkerEvent::Reconnect(generation, address, attempt) => {
                if generation != self.connection_generation
                    || self.manual_disconnect
                    || self.device.is_some()
                {
                    debug!(
                        "Ignoring stale Reconnect generation={generation} current={}",
                        self.connection_generation
                    );
                    return;
                }
                info!("Executing scheduled reconnect attempt {attempt} for {address}");
                self.handle_connect(address, true).await;
            }
        }
    }

    async fn start_scan(&mut self) {
        if self.device.is_some() || self.characteristic.is_some() {
            self.emit_state(ConnectionState::Error(
                "Disconnect from current controller before scanning.".into(),
            ));
            return;
        }

        // Cancel previous scan if running
        self.stop_scan(false).await;

        let Some(adapter) = self.adapter.clone() else {
            self.emit_state(ConnectionState::Error(
                "No Bluetooth adapter found. Check Bluetooth hardware and BlueZ.".into(),
            ));
            return;
        };

        // Check power state
        match adapter.is_powered().await {
            Ok(false) => {
                if let Err(error) = adapter.set_powered(true).await {
                    let message = format!(
                        "Bluetooth adapter is powered off or soft-blocked. Run 'rfkill unblock bluetooth': {error}"
                    );
                    error!("{message}");
                    self.emit_state(ConnectionState::Error(message));
                    return;
                }
            }
            Err(error) => warn!("Could not query adapter power state: {error}"),
            Ok(true) => {}
        }

        let filter = DiscoveryFilter {
            transport: DiscoveryTransport::Le,
            uuids: HashSet::new(),
            ..Default::default()
        };
        if let Err(error) = adapter.set_discovery_filter(filter).await {
            warn!("Could not set LE discovery filter, using BlueZ defaults: {error}");
        }

        let discover = match adapter.discover_devices().await {
            Ok(stream) => stream,
            Err(error) => {
                let message = format!("BLE scan failed: {error}");
                error!("{message}");
                self.emit_state(ConnectionState::Error(message));
                return;
            }
        };

        self.emit_state(ConnectionState::Scanning);
        info!("BLE cancellable scan started on adapter {}", adapter.name());

        let deadline = tokio::time::Instant::now() + SCAN_DURATION;
        self.active_scan = Some(ActiveScan {
            stream: Box::pin(discover),
            deadline,
        });
    }

    async fn stop_scan(&mut self, emit_complete: bool) {
        if self.active_scan.take().is_some() {
            if emit_complete {
                self.emit_event(BleEvent::ScanComplete);
                self.emit_state(ConnectionState::Disconnected);
                info!("BLE scan completed normally");
            } else {
                debug!("Active BLE scan cancelled");
            }
        }
    }

    async fn inspect_candidate(&self, adapter: &Adapter, address: Address) {
        let Ok(device) = adapter.device(address) else {
            return;
        };

        let name = match device.name().await.ok().flatten() {
            Some(name) => name,
            None => device.alias().await.unwrap_or_default(),
        };

        if !is_target_name(&name) {
            debug!("Skipping non-target BLE device {address} with name {name:?}");
            return;
        }

        let service_uuids = device
            .uuids()
            .await
            .ok()
            .flatten()
            .map(|uuids| uuids.into_iter().collect())
            .unwrap_or_default();
        let manufacturer_data = device
            .manufacturer_data()
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let service_data = device
            .service_data()
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let advertising_data = device
            .advertising_data()
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let rssi = device.rssi().await.ok().flatten();
        let tx_power = device.tx_power().await.ok().flatten();

        info!(
            "BLE candidate name={name:?} address={address} rssi={rssi:?} tx_power={tx_power:?} services={service_uuids:?}"
        );
        self.emit_state(ConnectionState::DeviceFound(name.clone()));
        self.emit_event(BleEvent::DeviceFound(DiscoveredDevice {
            address,
            name,
            rssi,
            tx_power,
            service_uuids,
            manufacturer_data,
            service_data,
            advertising_data,
        }));
    }

    async fn handle_connect(&mut self, address: Address, reconnect: bool) {
        self.connection_generation = self.connection_generation.wrapping_add(1);
        let generation = self.connection_generation;

        if self.device.is_some() || self.characteristic.is_some() {
            self.disconnect_current().await;
        }

        let Some(adapter) = self.adapter.clone() else {
            self.connection_failed("No Bluetooth adapter is available.".into(), reconnect)
                .await;
            return;
        };

        let device = match adapter.device(address) {
            Ok(device) => device,
            Err(error) => {
                self.connection_failed(
                    format!("Could not access device {address}: {error}"),
                    reconnect,
                )
                .await;
                return;
            }
        };

        let name = match device.name().await.ok().flatten() {
            Some(name) => name,
            None => device.alias().await.unwrap_or_else(|_| address.to_string()),
        };

        self.emit_state(if reconnect {
            ConnectionState::Reconnecting(name.clone(), self.reconnect_attempt.max(1))
        } else {
            ConnectionState::Connecting(name.clone())
        });
        info!("Connecting to BLE controller {name} at {address} (gen={generation})");

        // Wrapped in explicit connection timeout
        let connect_future = async {
            device.connect().await?;
            Ok::<(), bluer::Error>(())
        };

        match tokio::time::timeout(CONNECT_TIMEOUT, connect_future).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.connection_failed(
                    format!("Connection to {address} failed: {error}"),
                    reconnect,
                )
                .await;
                return;
            }
            Err(_) => {
                let _ = device.disconnect().await;
                self.connection_failed(
                    format!(
                        "Connection to {address} timed out after {}s",
                        CONNECT_TIMEOUT.as_secs()
                    ),
                    reconnect,
                )
                .await;
                return;
            }
        }

        self.emit_state(ConnectionState::DiscoveringServices(name.clone()));

        // GATT service discovery with timeout
        let discovery_future = Self::find_characteristic(&device);
        let (characteristic, write_mode) =
            match tokio::time::timeout(CONNECT_TIMEOUT, discovery_future).await {
                Ok(Ok(found)) => found,
                Ok(Err(error)) => {
                    let _ = device.disconnect().await;
                    self.connection_failed(
                        format!("GATT validation failed for {name}: {error}"),
                        reconnect,
                    )
                    .await;
                    return;
                }
                Err(_) => {
                    let _ = device.disconnect().await;
                    self.connection_failed(
                        format!("GATT service discovery for {name} timed out"),
                        reconnect,
                    )
                    .await;
                    return;
                }
            };

        // Ensure generation has not changed while waiting for I/O
        if generation != self.connection_generation {
            debug!("Connection generation changed during connect; cleaning up (gen={generation} current={})", self.connection_generation);
            let _ = device.disconnect().await;
            return;
        }

        self.device = Some(device.clone());
        self.characteristic = Some(characteristic);
        self.write_mode = Some(write_mode);
        self.reconnect_address = Some(address);
        self.reconnect_attempt = 0;
        self.cancel_reconnect();
        self.start_connection_monitor(device, generation).await;
        self.emit_state(ConnectionState::Ready(name.clone()));
        info!("BLE controller ready name={name} address={address} write_mode={write_mode:?} gen={generation}");
    }

    async fn find_characteristic(
        device: &Device,
    ) -> Result<(Characteristic, WriteMode), Box<dyn std::error::Error + Send + Sync>> {
        let services = device.services().await?;
        for service in services {
            let service_uuid = service.uuid().await?;
            let characteristics = service.characteristics().await?;
            debug!(
                "GATT service uuid={service_uuid} characteristic_count={}",
                characteristics.len()
            );

            if service_uuid != SERVICE_UUID {
                continue;
            }

            for characteristic in characteristics {
                let uuid = characteristic.uuid().await?;
                let flags = characteristic.flags().await?;
                debug!("GATT characteristic uuid={uuid} flags={flags:?}");

                if uuid != CHAR_UUID {
                    continue;
                }

                let write_mode = if flags.write_without_response {
                    WriteMode::WithoutResponse
                } else if flags.write {
                    WriteMode::WithResponse
                } else {
                    return Err(
                        "FFE1 has neither write nor write-without-response capability".into(),
                    );
                };
                return Ok((characteristic, write_mode));
            }
        }

        Err("expected service FFE0 and writable characteristic FFE1 were not found".into())
    }

    async fn start_connection_monitor(&mut self, device: Device, generation: u64) {
        if let Some(task) = self.monitor_task.take() {
            task.abort();
        }

        let events = match device.events().await {
            Ok(events) => events,
            Err(error) => {
                warn!("Could not subscribe to device events: {error}");
                return;
            }
        };

        let worker_tx = self.internal_tx.clone();
        self.monitor_task = Some(tokio::spawn(async move {
            let mut stream = Box::pin(events);
            while let Some(event) = stream.next().await {
                if let DeviceEvent::PropertyChanged(bluer::DeviceProperty::Connected(false)) = event
                {
                    info!("BlueZ reported device disconnected (gen={generation})");
                    let _ = worker_tx
                        .send(WorkerEvent::ConnectionLost(generation))
                        .await;
                    break;
                }
            }
        }));
    }

    async fn handle_command(&mut self, command: Command) {
        let Some(characteristic) = self.characteristic.as_ref() else {
            debug!("Ignoring command because controller is not connected: {command:?}");
            self.emit_event(BleEvent::CommandFailed(
                "Controller is not ready for commands.".into(),
            ));
            return;
        };
        let Some(write_mode) = self.write_mode else {
            self.emit_event(BleEvent::CommandFailed(
                "Command rejected: no GATT write mode selected.".into(),
            ));
            return;
        };

        let frame = match self.protocol.encode(&command) {
            Ok(frame) => frame,
            Err(error) => {
                self.emit_event(BleEvent::CommandFailed(error.to_string()));
                return;
            }
        };

        let device = self.device.clone();
        let name = match device {
            Some(device) => device
                .name()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "controller".into()),
            None => "controller".into(),
        };
        self.emit_state(ConnectionState::SendingCommand(name));

        let request = CharacteristicWriteRequest {
            op_type: match write_mode {
                WriteMode::WithResponse => WriteOp::Request,
                WriteMode::WithoutResponse => WriteOp::Command,
            },
            ..Default::default()
        };
        debug!("BLE write command={command:?} mode={write_mode:?} frame={frame:02X?}");

        match characteristic.write_ext(&frame, &request).await {
            Ok(()) => {
                self.emit_event(BleEvent::CommandSent);
                if let Some(device) = self.device.clone() {
                    if !device.is_connected().await.unwrap_or(false) && !self.manual_disconnect {
                        let _ = self
                            .internal_tx
                            .send(WorkerEvent::ConnectionLost(self.connection_generation))
                            .await;
                    } else if let Some(name) = device.name().await.ok().flatten() {
                        self.emit_state(ConnectionState::Ready(name));
                    }
                }
            }
            Err(error) => {
                let message = format!("BLE write failed using {write_mode:?}: {error}");
                error!("{message}");
                self.emit_event(BleEvent::CommandFailed(message));
                if let Some(device) = self.device.clone() {
                    if !device.is_connected().await.unwrap_or(false) && !self.manual_disconnect {
                        let _ = self
                            .internal_tx
                            .send(WorkerEvent::ConnectionLost(self.connection_generation))
                            .await;
                    }
                }
            }
        }
    }

    async fn connection_failed(&mut self, message: String, reconnect: bool) {
        error!("{message}");
        self.emit_state(ConnectionState::Error(message));
        if reconnect && !self.manual_disconnect {
            self.schedule_reconnect();
        }
    }

    fn schedule_reconnect(&mut self) {
        let Some(address) = self.reconnect_address else {
            return;
        };
        if self.reconnect_attempt >= MAX_RECONNECT_ATTEMPTS {
            self.emit_state(ConnectionState::Error(format!(
                "Reconnect limit reached for {address}; reconnect manually after checking power and BLE clients."
            )));
            return;
        }

        self.connection_generation = self.connection_generation.wrapping_add(1);
        let generation = self.connection_generation;

        self.reconnect_attempt += 1;
        let attempt = self.reconnect_attempt;
        let delay_seconds = 2_u64.saturating_pow((attempt - 1).min(4) as u32).min(30);
        self.emit_state(ConnectionState::Reconnecting(address.to_string(), attempt));

        if let Some(task) = self.reconnect_task.take() {
            task.abort();
        }

        let worker_tx = self.internal_tx.clone();
        self.reconnect_task = Some(tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds)).await;
            let _ = worker_tx
                .send(WorkerEvent::Reconnect(generation, address, attempt))
                .await;
        }));
        info!("Scheduled BLE reconnect attempt={attempt} delay_seconds={delay_seconds} (gen={generation})");
    }

    async fn disconnect_current(&mut self) {
        self.connection_generation = self.connection_generation.wrapping_add(1);
        if let Some(task) = self.monitor_task.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect_task.take() {
            task.abort();
        }
        self.characteristic = None;
        self.write_mode = None;
        if let Some(device) = self.device.take() {
            if device.is_connected().await.unwrap_or(false) {
                if let Err(error) = device.disconnect().await {
                    warn!("BLE disconnect failed: {error}");
                }
            }
        }
    }

    fn cancel_reconnect(&mut self) {
        self.connection_generation = self.connection_generation.wrapping_add(1);
        if let Some(task) = self.reconnect_task.take() {
            task.abort();
        }
    }

    fn emit_state(&self, state: ConnectionState) {
        self.emit_event(BleEvent::StateChanged(state));
    }

    fn emit_event(&self, event: BleEvent) {
        let _ = self.event_tx.send(event);
    }
}

enum ScanResult {
    Item(AdapterEvent),
    Deadline,
    Ended,
}

pub fn is_target_name(name: &str) -> bool {
    let upper = name.trim().to_ascii_uppercase();
    DEVICE_NAME_PREFIXES
        .iter()
        .any(|prefix| upper.starts_with(&prefix.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_target_name() {
        assert!(is_target_name("LEDDMX-03-A1B2"));
        assert!(is_target_name("LEDDMX"));
        assert!(is_target_name("leddmx-test"));
        assert!(is_target_name("LED DMX Strip"));
        assert!(is_target_name("LED-DMX-01"));
        assert!(!is_target_name("Govee-Light"));
        assert!(!is_target_name("Bluetooth Speaker"));
        assert!(!is_target_name(""));
    }
}
