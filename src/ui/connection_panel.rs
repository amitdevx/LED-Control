// Connection panel: device scanning, selection, and status display.

use crate::ble::{BleMessage, ConnectionState, DiscoveredDevice};
use gtk::prelude::*;
use gtk::{Box, Button, Label, ListBox, ListBoxRow, Orientation, Spinner};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

/// Build the connection panel widget.
///
/// Returns the container box and a handle to update the device list
/// and connection status from the main event loop.
pub fn create_connection_panel(ble_tx: mpsc::Sender<BleMessage>) -> (Box, ConnectionPanelHandle) {
    let container = Box::new(Orientation::Vertical, 8);
    container.add_css_class("connection-panel");

    // Status row
    let status_row = Box::new(Orientation::Horizontal, 8);
    status_row.set_halign(gtk::Align::Fill);

    let status_icon = Label::new(Some(""));
    status_icon.add_css_class("status-icon");
    status_icon.add_css_class("disconnected");

    let status_label = Label::new(Some("Disconnected"));
    status_label.add_css_class("status-label");
    status_label.set_hexpand(true);
    status_label.set_halign(gtk::Align::Start);
    status_label.set_wrap(true);
    status_label.set_max_width_chars(45);

    let spinner = Spinner::new();
    spinner.set_visible(false);

    status_row.append(&status_icon);
    status_row.append(&status_label);
    status_row.append(&spinner);
    container.append(&status_row);

    // Scan button
    let scan_btn = Button::with_label("Scan for Devices");
    scan_btn.add_css_class("scan-btn");
    scan_btn.add_css_class("suggested-action");

    let ble_tx_scan = ble_tx.clone();
    scan_btn.connect_clicked(move |_| {
        let _ = ble_tx_scan.try_send(BleMessage::Scan);
    });
    container.append(&scan_btn);

    // Device list
    let list_label = Label::new(Some("Available Devices"));
    list_label.add_css_class("section-label");
    list_label.set_halign(gtk::Align::Start);
    list_label.set_margin_top(4);
    container.append(&list_label);

    let device_list = ListBox::new();
    device_list.add_css_class("device-list");
    device_list.set_selection_mode(gtk::SelectionMode::Single);
    let placeholder_lbl = Label::new(Some("No devices found. Click Scan."));
    placeholder_lbl.set_margin_top(16);
    placeholder_lbl.set_margin_bottom(16);
    placeholder_lbl.add_css_class("dim-label");
    device_list.set_placeholder(Some(&placeholder_lbl));

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&device_list));
    scrolled.set_min_content_height(80);
    scrolled.set_max_content_height(160);
    scrolled.set_vexpand(false);
    container.append(&scrolled);

    // Connect / Disconnect buttons
    let btn_row = Box::new(Orientation::Horizontal, 8);
    btn_row.set_halign(gtk::Align::End);
    btn_row.set_margin_top(4);

    let connect_btn = Button::with_label("Connect");
    connect_btn.add_css_class("suggested-action");
    connect_btn.set_sensitive(false);

    let disconnect_btn = Button::with_label("Disconnect");
    disconnect_btn.add_css_class("destructive-action");
    disconnect_btn.set_sensitive(false);

    btn_row.append(&connect_btn);
    btn_row.append(&disconnect_btn);
    container.append(&btn_row);

    // Track discovered devices
    let devices: Rc<RefCell<Vec<DiscoveredDevice>>> = Rc::new(RefCell::new(Vec::new()));

    // Enable connect button when a device is selected
    let connect_btn_ref = connect_btn.clone();
    device_list.connect_row_selected(move |_, row| {
        connect_btn_ref.set_sensitive(row.is_some());
    });

    // Connect button handler
    let devices_connect = devices.clone();
    let ble_tx_connect = ble_tx.clone();
    let device_list_ref = device_list.clone();
    connect_btn.connect_clicked(move |_| {
        if let Some(row) = device_list_ref.selected_row() {
            let idx = row.index() as usize;
            let devs = devices_connect.borrow();
            if let Some(dev) = devs.get(idx) {
                let _ = ble_tx_connect.try_send(BleMessage::Connect(dev.address));
            }
        }
    });

    // Disconnect button handler
    let ble_tx_disconnect = ble_tx.clone();
    disconnect_btn.connect_clicked(move |_| {
        let _ = ble_tx_disconnect.try_send(BleMessage::Disconnect);
    });

    let handle = ConnectionPanelHandle {
        status_icon,
        status_label,
        spinner,
        scan_btn,
        connect_btn,
        disconnect_btn,
        device_list,
        devices,
    };

    (container, handle)
}

/// Handle for updating the connection panel from the main event loop.
pub struct ConnectionPanelHandle {
    status_icon: Label,
    status_label: Label,
    spinner: Spinner,
    scan_btn: Button,
    connect_btn: Button,
    disconnect_btn: Button,
    device_list: ListBox,
    devices: Rc<RefCell<Vec<DiscoveredDevice>>>,
}

impl ConnectionPanelHandle {
    /// Update the displayed connection state.
    pub fn set_state(&self, state: &ConnectionState) {
        // Remove old state classes
        for class in &[
            "disconnected",
            "scanning",
            "connecting",
            "connected",
            "error",
        ] {
            self.status_icon.remove_css_class(class);
        }

        match state {
            ConnectionState::Disconnected => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("disconnected");
                self.status_label.set_text("Disconnected");
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.scan_btn.set_sensitive(true);
                self.connect_btn
                    .set_sensitive(self.device_list.selected_row().is_some());
                self.disconnect_btn.set_sensitive(false);
            }
            ConnectionState::Scanning => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("scanning");
                self.status_label.set_text("Scanning...");
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.scan_btn.set_sensitive(false);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(false);
                // Clear previous results
                self.clear_devices();
            }
            ConnectionState::Connecting(name) => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("connecting");
                self.status_label
                    .set_text(&format!("Connecting to {}...", name));
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.scan_btn.set_sensitive(false);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(true);
            }
            ConnectionState::Ready(name) | ConnectionState::SendingCommand(name) => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("connected");
                self.status_label.set_text(&format!("Connected: {}", name));
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.scan_btn.set_sensitive(false);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(true);
            }
            ConnectionState::DiscoveringServices(name) => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("connecting");
                self.status_label
                    .set_text(&format!("Discovering services on {name}..."));
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.scan_btn.set_sensitive(false);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(true);
            }
            ConnectionState::Reconnecting(name, attempt) => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("connecting");
                self.status_label
                    .set_text(&format!("Reconnecting to {name} (attempt {attempt})..."));
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.scan_btn.set_sensitive(false);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(true);
            }
            ConnectionState::Error(msg) => {
                self.status_icon.set_text("");
                self.status_icon.add_css_class("error");
                self.status_label.set_text(&format!("Error: {}", msg));
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.scan_btn.set_sensitive(true);
                self.connect_btn.set_sensitive(false);
                self.disconnect_btn.set_sensitive(false);
            }
        }
    }

    /// Add a discovered device to the list.
    pub fn add_device(&self, device: DiscoveredDevice) {
        // Check for duplicates
        {
            let devs = self.devices.borrow();
            if devs.iter().any(|d| d.address == device.address) {
                return;
            }
        }

        self.status_label
            .set_text(&format!("Scanning... found {}", device.name));

        let row = ListBoxRow::new();
        let row_box = Box::new(Orientation::Horizontal, 8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);

        let name_label = Label::new(Some(&device.name));
        name_label.set_hexpand(true);
        name_label.set_halign(gtk::Align::Start);
        name_label.add_css_class("device-name");

        let addr_label = Label::new(Some(&device.address.to_string()));
        addr_label.add_css_class("device-addr");

        let rssi_text = device
            .rssi
            .map(|r| format!("{} dBm", r))
            .unwrap_or_else(|| "--".into());
        let rssi_label = Label::new(Some(&rssi_text));
        rssi_label.add_css_class("device-rssi");

        row_box.append(&name_label);
        row_box.append(&addr_label);
        row_box.append(&rssi_label);
        row.set_child(Some(&row_box));
        let diagnostic_summary = format!(
            "GATT UUIDs: {} | manufacturer fields: {} | service fields: {} | advertising fields: {} | TX power: {:?}",
            device.service_uuids.len(),
            device.manufacturer_data.len(),
            device.service_data.len(),
            device.advertising_data.len(),
            device.tx_power,
        );
        row.set_tooltip_text(Some(&diagnostic_summary));

        self.device_list.append(&row);
        self.devices.borrow_mut().push(device);
    }

    /// Clear the device list.
    fn clear_devices(&self) {
        self.devices.borrow_mut().clear();
        while let Some(row) = self.device_list.row_at_index(0) {
            self.device_list.remove(&row);
        }
    }
}
