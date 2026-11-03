// ponytail: Settings panel for channel calibration, electrical protection, and troubleshooting.

use crate::ble::{BleMessage, ColorOrder, Command};
use gtk::prelude::*;
use gtk::{Box, Button, DropDown, Label, Orientation, StringList, Switch};
use std::rc::Rc;
use tokio::sync::mpsc;

/// Build the settings panel widget.
pub fn create_settings_panel(
    ble_tx: mpsc::Sender<BleMessage>,
    initial_order: ColorOrder,
    initial_power_limit: bool,
    on_settings_change: impl Fn(ColorOrder, bool) + Clone + 'static,
) -> Box {
    let container = Box::new(Orientation::Vertical, 10);
    container.add_css_class("settings-panel");

    let current_order = Rc::new(std::cell::Cell::new(initial_order));
    let current_power_limit = Rc::new(std::cell::Cell::new(initial_power_limit));

    // Color order selector
    let order_label = Label::new(Some("Color Channel Order"));
    order_label.add_css_class("section-label");
    order_label.set_halign(gtk::Align::Start);
    container.append(&order_label);

    let order_desc = Label::new(Some(
        "Different strip models wire color channels differently. \
         The One94Store SR-SMD-SM uses RBG order (green and blue swapped). \
         Use the test buttons below to calibrate.",
    ));
    order_desc.add_css_class("dim-label");
    order_desc.set_halign(gtk::Align::Start);
    order_desc.set_wrap(true);
    container.append(&order_desc);

    let orders = ColorOrder::all();
    let order_names: Vec<&str> = orders.iter().map(|o| o.label()).collect();
    let order_list = StringList::new(&order_names);
    let order_dropdown = DropDown::new(Some(order_list), gtk::Expression::NONE);

    // Set initial selection
    let initial_idx = orders.iter().position(|&o| o == initial_order).unwrap_or(0);
    order_dropdown.set_selected(initial_idx as u32);
    order_dropdown.add_css_class("effect-dropdown");

    let ble_tx_order = ble_tx.clone();
    let on_change_order = on_settings_change.clone();
    let order_cell = current_order.clone();
    let pwr_lim_cell = current_power_limit.clone();
    order_dropdown.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        let orders = ColorOrder::all();
        if let Some(order) = orders.get(idx) {
            order_cell.set(*order);
            let _ = ble_tx_order.try_send(BleMessage::SetColorOrder(*order));
            on_change_order(*order, pwr_lim_cell.get());
        }
    });
    container.append(&order_dropdown);

    // Hardware Calibration Test Buttons
    let test_label = Label::new(Some("Hardware Channel Test & Calibration"));
    test_label.add_css_class("section-label");
    test_label.set_halign(gtk::Align::Start);
    test_label.set_margin_top(10);
    container.append(&test_label);

    let test_desc = Label::new(Some(
        "Click a button to send a pure test color to the physical strip. \
         Confirm that Red lights up red, Green lights up green, and Blue lights up blue.",
    ));
    test_desc.add_css_class("dim-label");
    test_desc.set_halign(gtk::Align::Start);
    test_desc.set_wrap(true);
    container.append(&test_desc);

    let test_box = Box::new(Orientation::Horizontal, 8);
    test_box.set_margin_top(4);

    let test_red = Button::with_label("Test Red");
    test_red.add_css_class("power-off-btn");
    let tx_red = ble_tx.clone();
    test_red.connect_clicked(move |_| {
        let _ = tx_red.try_send(BleMessage::SendCommand(Command::PowerOn));
        let _ = tx_red.try_send(BleMessage::SendCommand(Command::StaticColor {
            r: 255,
            g: 0,
            b: 0,
        }));
    });

    let test_green = Button::with_label("Test Green");
    test_green.add_css_class("power-on-btn");
    let tx_green = ble_tx.clone();
    test_green.connect_clicked(move |_| {
        let _ = tx_green.try_send(BleMessage::SendCommand(Command::PowerOn));
        let _ = tx_green.try_send(BleMessage::SendCommand(Command::StaticColor {
            r: 0,
            g: 255,
            b: 0,
        }));
    });

    let test_blue = Button::with_label("Test Blue");
    test_blue.add_css_class("suggested-action");
    let tx_blue = ble_tx.clone();
    test_blue.connect_clicked(move |_| {
        let _ = tx_blue.try_send(BleMessage::SendCommand(Command::PowerOn));
        let _ = tx_blue.try_send(BleMessage::SendCommand(Command::StaticColor {
            r: 0,
            g: 0,
            b: 255,
        }));
    });

    let test_white = Button::with_label("Test White");
    test_white.add_css_class("pattern-btn");
    let tx_white = ble_tx.clone();
    test_white.connect_clicked(move |_| {
        let _ = tx_white.try_send(BleMessage::SendCommand(Command::PowerOn));
        let _ = tx_white.try_send(BleMessage::SendCommand(Command::StaticColor {
            r: 255,
            g: 255,
            b: 255,
        }));
    });

    test_box.append(&test_red);
    test_box.append(&test_green);
    test_box.append(&test_blue);
    test_box.append(&test_white);
    container.append(&test_box);

    // Hardware Electrical Safety section
    let pwr_label = Label::new(Some("Software Effect Output Limit"));
    pwr_label.add_css_class("section-label");
    pwr_label.set_halign(gtk::Align::Start);
    pwr_label.set_margin_top(14);
    container.append(&pwr_label);

    let pwr_row = Box::new(Orientation::Horizontal, 10);
    pwr_row.set_margin_top(4);

    let pwr_switch = Switch::new();
    pwr_switch.set_active(initial_power_limit);
    pwr_switch.set_valign(gtk::Align::Center);

    let pwr_expl = Label::new(Some(
        "Limits software-effect output to a conservative channel sum when enabled. Static colour commands preserve the requested RGB bytes; physical power safety still depends on the strip power supply.",
    ));
    pwr_expl.add_css_class("dim-label");
    pwr_expl.set_halign(gtk::Align::Start);
    pwr_expl.set_wrap(true);
    pwr_expl.set_hexpand(true);

    let on_change_pwr = on_settings_change;
    let order_cell_pwr = current_order;
    let pwr_cell = current_power_limit;
    pwr_switch.connect_active_notify(move |sw| {
        let active = sw.is_active();
        pwr_cell.set(active);
        on_change_pwr(order_cell_pwr.get(), active);
    });

    pwr_row.append(&pwr_switch);
    pwr_row.append(&pwr_expl);
    container.append(&pwr_row);

    // Bluetooth Troubleshooting Guide
    let bt_label = Label::new(Some("Bluetooth Troubleshooting"));
    bt_label.add_css_class("section-label");
    bt_label.set_halign(gtk::Align::Start);
    bt_label.set_margin_top(14);
    container.append(&bt_label);

    let bt_help = Label::new(Some(
        "1. Controller Limit: The strip only accepts ONE BLE connection at a time. Close the LED LAMP phone app before connecting.\n\
         2. Bluetooth blocked: If you see 'Resource Not Ready', your adapter may be blocked. Run:\n\
            sudo rfkill unblock bluetooth\n\
         3. Restart BlueZ: If discovery fails, run:\n\
            sudo systemctl restart bluetooth\n\
         4. Hardware persistence: Pattern and static color remain active in controller hardware after closing this app.",
    ));
    bt_help.add_css_class("dim-label");
    bt_help.set_halign(gtk::Align::Start);
    bt_help.set_wrap(true);
    container.append(&bt_help);

    // About section
    let about_label = Label::new(Some("Device Info"));
    about_label.add_css_class("section-label");
    about_label.set_halign(gtk::Align::Start);
    about_label.set_margin_top(12);
    container.append(&about_label);

    let about_text = Label::new(Some(
        "Model: One94Store SR-SMD-SM (ASIN: B0FTQ5Q7BZ)\n\
         Length: 5 Metres / Flexible SMD 5050 RGB Strip\n\
         Supported Controllers: MELK / ELK-BLEDOM (0xFFF0/0xFFF3) & LEDDMX-* (0xFFE0/0xFFE1)\n\
         Protocols: 9-byte frames (0x7E...0xEF / 0x7B...0xBF)\n\
         Auto-detection: Active on connection",
    ));
    about_text.add_css_class("dim-label");
    about_text.set_halign(gtk::Align::Start);
    about_text.set_wrap(true);
    container.append(&about_text);

    container
}
