// ponytail: BLE worker managing BlueZ communication, cancellable scanning,
// connection lifecycle, bounded reconnects, and serialized GATT writes.

mod protocol;

pub use protocol::{ColorOrder, Command, Protocol, ProtocolFamily};

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

const FFF0_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000fff0_0000_1000_8000_00805f9b34fb);
const FFF3_CHAR_UUID: Uuid = Uuid::from_u128(0x0000fff3_0000_1000_8000_00805f9b34fb);

const FFE0_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000ffe0_0000_1000_8000_00805f9b34fb);
const FFE1_CHAR_UUID: Uuid = Uuid::from_u128(0x0000ffe1_0000_1000_8000_00805f9b34fb);

const DEVICE_NAME_PREFIXES: &[&str] = &["MELK", "ELK", "LEDDMX", "LED DMX", "LED-DMX"];
const MAX_RECONNECT_ATTEMPTS: u8 = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SCAN_DURATION: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Scanning,
    Connecting(String),
    DiscoveringServices(String),
    Ready(String),
    #[allow(dead_code)]
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
    CommandSent { with_response: bool },
    CommandFailed(String),
}

#[derive(Debug)]
enum WorkerEvent {
    ConnectionLost(u64),
    Reconnect(u64, Address, u8),
    Connected {
        generation: u64,
        device: Device,
        characteristic: Characteristic,
        write_mode: WriteMode,
        name: String,
        family: ProtocolFamily,
    },
    ConnectFailed {
        generation: u64,
        error: String,
        reconnect: bool,
    },
}

#[derive(Clone)]
pub struct BleHandle {
    tx: mpsc::Sender<BleMessage>,
    priority_tx: mpsc::Sender<BleMessage>,
}

impl BleHandle {
    pub fn try_send(&self, message: BleMessage) {
        match message {
            BleMessage::Disconnect
            | BleMessage::Shutdown
            | BleMessage::Connect(_)
            | BleMessage::SendCommand(Command::PowerOn)
            | BleMessage::SendCommand(Command::PowerOff) => {
                if let Err(error) = self.priority_tx.try_send(message) {
                    warn!("BLE priority queue rejected message: {error}");
                }
            }
            _ => {
                if let Err(error) = self.tx.try_send(message) {
                    debug!("BLE worker streaming queue skipped frame: {error}");
                }
            }
        }
    }

    pub fn sender(&self) -> mpsc::Sender<BleMessage> {
        self.priority_tx.clone()
    }
}

struct ActiveScan {
    stream: Pin<Box<dyn futures::Stream<Item = AdapterEvent> + Send>>,
    deadline: tokio::time::Instant,
}

pub struct BleWorker {
    rx: mpsc::Receiver<BleMessage>,
    priority_rx: mpsc::Receiver<BleMessage>,
    internal_rx: mpsc::Receiver<WorkerEvent>,
    internal_tx: mpsc::Sender<WorkerEvent>,
    event_tx: Sender<BleEvent>,
    session: Option<Session>,
    adapter: Option<Adapter>,
    device: Option<Device>,
    characteristic: Option<Characteristic>,
    write_mode: Option<WriteMode>,
    active_scan: Option<ActiveScan>,
    connect_task: Option<JoinHandle<()>>,
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
        let (tx, rx) = mpsc::channel(64);
        let (priority_tx, priority_rx) = mpsc::channel(16);
        let (internal_tx, internal_rx) = mpsc::channel(16);
        let handle = BleHandle { tx, priority_tx };
        let worker = Self {
            rx,
            priority_rx,
            internal_rx,
            internal_tx,
            event_tx,
            session: None,
            adapter: None,
            device: None,
            characteristic: None,
            write_mode: None,
            active_scan: None,
            connect_task: None,
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
                biased;

                priority_message = self.priority_rx.recv() => {
                    let Some(message) = priority_message else { break };
                    if !self.handle_message(message).await {
                        break;
                    }
                }
                event = self.internal_rx.recv() => {
                    if let Some(event) = event {
                        self.handle_worker_event(event).await;
                    }
                }
                message = self.rx.recv() => {
                    let Some(message) = message else { break };
                    if !self.handle_message(message).await {
                        break;
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
        self.cancel_connect();
        self.cancel_reconnect();
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
                self.start_connect_task(address, false);
            }
            BleMessage::Disconnect => {
                self.stop_scan(false).await;
                self.manual_disconnect = true;
                self.cancel_connect();
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
                self.cancel_connect();
                self.cancel_reconnect();
                self.disconnect_current().await;
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
                self.start_connect_task(address, true);
            }
            WorkerEvent::Connected {
                generation,
                device,
                characteristic,
                write_mode,
                name,
                family,
            } => {
                if generation != self.connection_generation || self.manual_disconnect {
                    debug!(
                        "Ignoring stale Connected event (gen={generation}, current={})",
                        self.connection_generation
                    );
                    let _ = device.disconnect().await;
                    return;
                }
                self.connect_task = None;
                self.device = Some(device.clone());
                self.characteristic = Some(characteristic);
                self.write_mode = Some(write_mode);
                self.protocol.set_family(family);
                let address = device.address();
                self.reconnect_address = Some(address);
                self.reconnect_attempt = 0;
                self.cancel_reconnect();
                self.start_connection_monitor(device, generation).await;
                self.emit_state(ConnectionState::Ready(name.clone()));
                info!(
                    "BLE controller ready name={name} address={address} write_mode={write_mode:?} family={family:?} gen={generation}"
                );
            }
            WorkerEvent::ConnectFailed {
                generation,
                error,
                reconnect,
            } => {
                if generation != self.connection_generation || self.manual_disconnect {
                    debug!(
                        "Ignoring stale ConnectFailed event (gen={generation}, current={})",
                        self.connection_generation
                    );
                    return;
                }
                self.connect_task = None;
                self.connection_failed(error, reconnect).await;
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
        // ponytail: Do not emit ConnectionState::DeviceFound; state remains Scanning while discovery stream is active
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

    fn cancel_connect(&mut self) {
        if let Some(task) = self.connect_task.take() {
            debug!("Aborting in-flight connect task");
            task.abort();
        }
    }

    fn start_connect_task(&mut self, address: Address, reconnect: bool) {
        self.cancel_connect();
        self.cancel_reconnect();
        self.connection_generation = self.connection_generation.wrapping_add(1);
        let generation = self.connection_generation;

        let Some(adapter) = self.adapter.clone() else {
            let error = "No Bluetooth adapter is available.".to_string();
            let _ = self.internal_tx.try_send(WorkerEvent::ConnectFailed {
                generation,
                error,
                reconnect,
            });
            return;
        };

        let internal_tx = self.internal_tx.clone();
        let reconnect_attempt = self.reconnect_attempt;
        let event_tx = self.event_tx.clone();

        self.connect_task = Some(tokio::spawn(async move {
            let device = match adapter.device(address) {
                Ok(device) => device,
                Err(error) => {
                    let _ = internal_tx
                        .send(WorkerEvent::ConnectFailed {
                            generation,
                            error: format!("Could not access device {address}: {error}"),
                            reconnect,
                        })
                        .await;
                    return;
                }
            };

            let name = match device.name().await.ok().flatten() {
                Some(name) => name,
                None => device.alias().await.unwrap_or_else(|_| address.to_string()),
            };

            let _ = event_tx.send(BleEvent::StateChanged(if reconnect {
                ConnectionState::Reconnecting(name.clone(), reconnect_attempt.max(1))
            } else {
                ConnectionState::Connecting(name.clone())
            }));
            info!("Connecting to BLE controller {name} at {address} (gen={generation})");

            let connect_future = async {
                if device.name().await.is_err() {
                    if let Ok(discover) = adapter.discover_devices().await {
                        for _ in 0..10 {
                            if device.name().await.is_ok() {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                        drop(discover);
                    }
                }
                device.connect().await?;
                for _ in 0..15 {
                    if device.is_services_resolved().await.unwrap_or(false) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                Ok::<(), bluer::Error>(())
            };

            match tokio::time::timeout(CONNECT_TIMEOUT, connect_future).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let _ = device.disconnect().await;
                    let _ = internal_tx
                        .send(WorkerEvent::ConnectFailed {
                            generation,
                            error: format!("Connection to {address} failed: {error}"),
                            reconnect,
                        })
                        .await;
                    return;
                }
                Err(_) => {
                    let _ = device.disconnect().await;
                    let _ = internal_tx
                        .send(WorkerEvent::ConnectFailed {
                            generation,
                            error: format!(
                                "Connection to {address} timed out after {}s",
                                CONNECT_TIMEOUT.as_secs()
                            ),
                            reconnect,
                        })
                        .await;
                    return;
                }
            }

            let _ = event_tx.send(BleEvent::StateChanged(
                ConnectionState::DiscoveringServices(name.clone()),
            ));

            let discovery_future = Self::find_characteristic(&device);
            match tokio::time::timeout(CONNECT_TIMEOUT, discovery_future).await {
                Ok(Ok((characteristic, write_mode, family))) => {
                    let _ = internal_tx
                        .send(WorkerEvent::Connected {
                            generation,
                            device,
                            characteristic,
                            write_mode,
                            name,
                            family,
                        })
                        .await;
                }
                Ok(Err(error)) => {
                    let _ = device.disconnect().await;
                    let _ = internal_tx
                        .send(WorkerEvent::ConnectFailed {
                            generation,
                            error: format!("GATT validation failed for {name}: {error}"),
                            reconnect,
                        })
                        .await;
                }
                Err(_) => {
                    let _ = device.disconnect().await;
                    let _ = internal_tx
                        .send(WorkerEvent::ConnectFailed {
                            generation,
                            error: format!("GATT service discovery for {name} timed out"),
                            reconnect,
                        })
                        .await;
                }
            }
        }));
    }

    async fn find_characteristic(
        device: &Device,
    ) -> Result<(Characteristic, WriteMode, ProtocolFamily), Box<dyn std::error::Error + Send + Sync>>
    {
        let services = device.services().await?;

        // Priority 1: Check for FFF0 service and FFF3 write characteristic (MELK / ELK-BLEDOM)
        for service in &services {
            let service_uuid = service.uuid().await?;
            if service_uuid == FFF0_SERVICE_UUID {
                let characteristics = service.characteristics().await?;
                for characteristic in characteristics {
                    let uuid = characteristic.uuid().await?;
                    if uuid == FFF3_CHAR_UUID {
                        let flags = characteristic.flags().await?;
                        let write_mode = if flags.write_without_response {
                            WriteMode::WithoutResponse
                        } else if flags.write {
                            WriteMode::WithResponse
                        } else {
                            continue;
                        };
                        return Ok((characteristic, write_mode, ProtocolFamily::ElkBledom));
                    }
                }
            }
        }

        // Priority 2: Check for FFE0 service and FFE1 write characteristic (LEDDMX)
        for service in &services {
            let service_uuid = service.uuid().await?;
            if service_uuid == FFE0_SERVICE_UUID {
                let characteristics = service.characteristics().await?;
                for characteristic in characteristics {
                    let uuid = characteristic.uuid().await?;
                    if uuid == FFE1_CHAR_UUID {
                        let flags = characteristic.flags().await?;
                        let write_mode = if flags.write_without_response {
                            WriteMode::WithoutResponse
                        } else if flags.write {
                            WriteMode::WithResponse
                        } else {
                            continue;
                        };
                        return Ok((characteristic, write_mode, ProtocolFamily::Leddmdx));
                    }
                }
            }
        }

        // Priority 3: Check for any writable characteristic under FFF0 or FFE0
        for service in &services {
            let service_uuid = service.uuid().await?;
            let family = if service_uuid == FFF0_SERVICE_UUID {
                ProtocolFamily::ElkBledom
            } else if service_uuid == FFE0_SERVICE_UUID {
                ProtocolFamily::Leddmdx
            } else {
                continue;
            };

            let characteristics = service.characteristics().await?;
            for characteristic in characteristics {
                let flags = characteristic.flags().await?;
                if flags.write_without_response || flags.write {
                    let write_mode = if flags.write_without_response {
                        WriteMode::WithoutResponse
                    } else {
                        WriteMode::WithResponse
                    };
                    return Ok((characteristic, write_mode, family));
                }
            }
        }

        Err(
            "No compatible writable GATT characteristic found (checked FFF0/FFF3 and FFE0/FFE1)"
                .into(),
        )
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
                let with_response = write_mode == WriteMode::WithResponse;
                self.emit_event(BleEvent::CommandSent { with_response });
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
        self.cancel_connect();
        self.cancel_reconnect();
        self.connection_generation = self.connection_generation.wrapping_add(1);
        if let Some(task) = self.monitor_task.take() {
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
        assert!(is_target_name("MELK-OA10   45"));
        assert!(is_target_name("ELK-BLEDOM"));
        assert!(is_target_name("elk-bledom-01"));
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
