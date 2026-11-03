// ponytail: LED Controller for MELK-OA10 Smart RGB LED Strip Light (ELK-BLEDOM family).
// High-performance native Linux desktop GUI using GTK4, Cairo, and BlueZ (bluer).
// Strictly zero emojis and zero em-dashes.

mod ble;
mod config;
mod effects;
mod ui;

use ble::{Address, BleEvent, BleMessage, BleWorker, ColorOrder, Command, ConnectionState};
use config::AppConfig;
use effects::{Effect, EffectEngine, MusicEngine};
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Box, Button, HeaderBar, Notebook, Orientation};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const APP_ID: &str = "com.amitdevx.ledcontroller";

fn main() -> glib::ExitCode {
    env_logger::init();

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    // Load embedded dark theme CSS on application display
    if let Some(display) = gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(ui::app_css());
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // Load persisted user configuration
    let config = Rc::new(RefCell::new(AppConfig::load()));
    let _ = config.borrow().save();

    // Set up standard mpsc channel for worker -> UI communication
    let (event_tx, event_rx) = std::sync::mpsc::channel::<BleEvent>();

    // Create BLE worker and handle
    let (ble_handle, ble_worker) = BleWorker::new(event_tx);
    let ble_shutdown = ble_handle.clone();

    app.connect_shutdown(move |_| {
        ble_shutdown.try_send(BleMessage::Shutdown);
    });
    // Run tokio runtime on a dedicated background thread for BLE I/O
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async move {
            ble_worker.run().await;
        });
    });

    let window = ApplicationWindow::builder()
        .application(app)
        .title("MELK-OA10 Smart RGB LED Controller")
        .default_width(680)
        .default_height(820)
        .build();

    let root_box = Box::new(Orientation::Vertical, 10);
    root_box.add_css_class("main-container");

    let cfg_init = config.borrow().clone();

    // Power state tracking
    let powered = Rc::new(Cell::new(cfg_init.powered));

    // Header bar with power toggle button and title
    let header = HeaderBar::new();
    let power_btn = Button::new();
    if cfg_init.powered {
        power_btn.set_label("Power: ON");
        power_btn.add_css_class("power-on-btn");
    } else {
        power_btn.set_label("Power: OFF");
        power_btn.add_css_class("power-off-btn");
    }
    power_btn.set_tooltip_text(Some("Click to toggle strip power on or off"));
    header.pack_end(&power_btn);
    window.set_titlebar(Some(&header));

    // Animated LED Strip widget
    let (strip_widget, strip_state) = ui::create_strip_widget();
    {
        let mut s = strip_state.borrow_mut();
        s.desired_power = cfg_init.powered;
        s.powered = cfg_init.powered;
        s.physical_connected = false;
        s.brightness = cfg_init.brightness as f64 / 100.0;
        s.set_all_leds(
            cfg_init.color_r as f64 / 255.0,
            cfg_init.color_g as f64 / 255.0,
            cfg_init.color_b as f64 / 255.0,
        );
    }
    root_box.append(&strip_widget);

    // Track whether a physical BLE strip is actively connected
    let is_connected = Rc::new(Cell::new(false));

    // Effect and Music engine state
    let engine = Rc::new(RefCell::new(EffectEngine::new()));
    let music_engine = Rc::new(RefCell::new(MusicEngine::new()));
    let music_active = Rc::new(Cell::new(false));

    // Initialize engine parameters from config
    {
        let mut eng = engine.borrow_mut();
        eng.set_initial_color(
            cfg_init.color_r,
            cfg_init.color_g,
            cfg_init.color_b,
            cfg_init.brightness as f64 / 100.0,
        );
        let mut p = eng.params().clone();
        p.power_limit_enabled = cfg_init.power_limit;
        eng.set_params(p);

        if let Some(pat_idx) = cfg_init.hardware_pattern {
            eng.set_effect(Effect::HardwarePattern(pat_idx));
            strip_state.borrow_mut().effect_active = true;
        }
    }

    // Configure initial BLE parameters
    let initial_order = ColorOrder::from_str_name(&cfg_init.color_order).unwrap_or_else(|| {
        log::warn!(
            "Unrecognized color order '{}' in config, falling back to default RGB",
            cfg_init.color_order
        );
        ColorOrder::default()
    });
    ble_handle.try_send(BleMessage::SetColorOrder(initial_order));

    // Auto-connect to saved MELK-OA10 LED controller on launch
    if let Some(ref addr_str) = cfg_init.last_device_address {
        if let Ok(addr) = addr_str.parse::<Address>() {
            log::info!("Auto-connecting to saved MELK-OA10 controller at {addr}");
            ble_handle.try_send(BleMessage::Connect(addr));
        }
    }

    // Helper to automatically wake / turn on power if user interacts with controls
    let ensure_powered = {
        let powered = powered.clone();
        let power_btn = power_btn.clone();
        let strip = strip_state.clone();
        let ble = ble_handle.clone();
        let is_conn = is_connected.clone();
        let cfg = config.clone();

        Rc::new(move || {
            if !powered.get() {
                powered.set(true);
                power_btn.set_label("Power: ON");
                power_btn.remove_css_class("power-off-btn");
                power_btn.add_css_class("power-on-btn");
                {
                    let mut s = strip.borrow_mut();
                    s.desired_power = true;
                    s.powered = true;
                    s.physical_connected = is_conn.get();
                }
                cfg.borrow_mut().powered = true;
                let _ = cfg.borrow().save();
                if is_conn.get() {
                    ble.try_send(BleMessage::SendCommand(Command::PowerOn));
                }
            }
        })
    };

    // Master Brightness Control Bar (Directly below strip preview, visible globally across all modes)
    let strip_state_br = strip_state.clone();
    let engine_br = engine.clone();
    let ble_handle_br = ble_handle.clone();
    let is_connected_br = is_connected.clone();
    let cfg_br = config.clone();
    let ensure_pwr_br = ensure_powered.clone();

    let (brightness_box, _brightness_handle) =
        ui::create_brightness_control(cfg_init.brightness, move |pct| {
            if pct > 0 {
                ensure_pwr_br();
            }
            strip_state_br.borrow_mut().brightness = pct as f64 / 100.0;
            engine_br
                .borrow_mut()
                .set_target_brightness(pct as f64 / 100.0);
            cfg_br.borrow_mut().brightness = pct;
            let _ = cfg_br.borrow().save();

            if is_connected_br.get() {
                let eff = engine_br.borrow().effect();
                if matches!(eff, Effect::HardwarePattern(_)) {
                    ble_handle_br.try_send(BleMessage::SendCommand(Command::Brightness(pct)));
                }
            }
        });
    root_box.append(&brightness_box);

    // Notebook tabs for clean organization
    let notebook = Notebook::new();

    // 1. Color Picker tab
    let strip_state_col = strip_state.clone();
    let engine_col = engine.clone();
    let music_act_col = music_active.clone();
    let ensure_pwr_col = ensure_powered.clone();
    let cfg_col = config.clone();
    let is_conn_col = is_connected.clone();
    let ble_col = ble_handle.clone();

    let (color_picker_box, color_handle) = ui::create_color_picker(move |r, g, b| {
        ensure_pwr_col();
        {
            let mut eng = engine_col.borrow_mut();
            eng.set_target_color(r, g, b);
            eng.set_effect(Effect::Static);
        }
        music_act_col.set(false);

        {
            let mut s = strip_state_col.borrow_mut();
            s.effect_active = true;
        }

        let had_hw = cfg_col.borrow().hardware_pattern.is_some();
        {
            let mut c = cfg_col.borrow_mut();
            c.color_r = r;
            c.color_g = g;
            c.color_b = b;
            c.effect_name = "Static".into();
            c.hardware_pattern = None;
            let _ = c.save();
        }

        if had_hw && is_conn_col.get() {
            ble_col.try_send(BleMessage::SendCommand(Command::Brightness(100)));
        }
    });

    // Initialize color picker handle from config
    (color_handle.sync_ui)(cfg_init.color_r, cfg_init.color_g, cfg_init.color_b);
    let col_tab_label = gtk::Label::new(Some("Colors"));
    notebook.append_page(&color_picker_box, Some(&col_tab_label));

    // 2. Hardware Modes tab (Verified built-in microchip routines running directly on MELK-OA10)
    let ble_handle_hw = ble_handle.clone();
    let engine_hw = engine.clone();
    let strip_state_hw = strip_state.clone();
    let is_connected_hw = is_connected.clone();
    let music_act_hw = music_active.clone();
    let ensure_pwr_hw = ensure_powered.clone();
    let cfg_hw = config.clone();

    let (hw_box, _hw_handle) = ui::create_hardware_modes_panel(move |idx| {
        if idx <= 210 {
            ensure_pwr_hw();
            music_act_hw.set(false);
            engine_hw
                .borrow_mut()
                .set_effect(Effect::HardwarePattern(idx));
            strip_state_hw.borrow_mut().effect_active = true;

            let cur_br = {
                let mut c = cfg_hw.borrow_mut();
                c.hardware_pattern = Some(idx);
                c.effect_name = format!("HardwarePattern({})", idx);
                let _ = c.save();
                c.brightness
            };

            if is_connected_hw.get() {
                ble_handle_hw.try_send(BleMessage::SendCommand(Command::Brightness(cur_br)));
                ble_handle_hw.try_send(BleMessage::SendCommand(Command::Pattern(idx)));
            } else {
                log::info!(
                    "Hardware pattern {idx} selected in local preview; will transmit to strip once BLE connects"
                );
            }
        }
    });
    let hw_tab_label = gtk::Label::new(Some("Hardware Modes"));
    notebook.append_page(&hw_box, Some(&hw_tab_label));

    // 3. Dynamic Effects tab (PC software streaming animations)
    let ble_handle_fx = ble_handle.clone();
    let engine_fx = engine.clone();
    let strip_state_fx = strip_state.clone();
    let is_connected_fx = is_connected.clone();
    let music_act_fx = music_active.clone();
    let ensure_pwr_fx = ensure_powered.clone();
    let cfg_fx = config.clone();

    let (effects_box, _fx_handle) = ui::create_dynamic_effects_panel(
        {
            let engine = engine_fx.clone();
            let strip = strip_state_fx.clone();
            let music_act = music_act_fx.clone();
            let ensure_pwr = ensure_pwr_fx.clone();
            let cfg = cfg_fx.clone();
            let is_conn = is_connected_fx.clone();
            let ble = ble_handle_fx.clone();
            move |eff| {
                ensure_pwr();
                music_act.set(false);
                engine.borrow_mut().set_effect(eff);
                strip.borrow_mut().effect_active = eff != Effect::Static;

                let had_hw = cfg.borrow().hardware_pattern.is_some();
                let mut c = cfg.borrow_mut();
                c.effect_name = eff.label().into();
                c.hardware_pattern = None;
                let _ = c.save();

                if had_hw && is_conn.get() {
                    ble.try_send(BleMessage::SendCommand(Command::Brightness(100)));
                }
            }
        },
        {
            let engine = engine_fx.clone();
            move |params| {
                engine.borrow_mut().set_params(params.clone());
            }
        },
    );
    let fx_tab_label = gtk::Label::new(Some("Dynamic Effects"));
    notebook.append_page(&effects_box, Some(&fx_tab_label));

    // 3. Music Sync tab
    let music_active_toggle = music_active.clone();
    let strip_state_music = strip_state.clone();
    let ensure_pwr_music = ensure_powered.clone();
    let music_box = ui::create_music_panel(music_engine.clone(), move |active| {
        if active {
            ensure_pwr_music();
        }
        music_active_toggle.set(active);
        strip_state_music.borrow_mut().effect_active = active;
    });
    let music_tab_label = gtk::Label::new(Some("Music Sync"));
    notebook.append_page(&music_box, Some(&music_tab_label));

    // 4. Connection & Devices tab
    let (conn_box, conn_handle) = ui::create_connection_panel(ble_handle.sender());
    let conn_tab_label = gtk::Label::new(Some("Devices"));
    notebook.append_page(&conn_box, Some(&conn_tab_label));

    // 5. Settings & Calibration tab
    let cfg_settings = config.clone();
    let engine_settings = engine.clone();
    let settings_box = ui::create_settings_panel(
        ble_handle.sender(),
        initial_order,
        cfg_init.power_limit,
        move |order, pwr_limit| {
            let mut c = cfg_settings.borrow_mut();
            c.color_order = order.label().split(' ').next().unwrap_or("RBG").into();
            c.power_limit = pwr_limit;
            let _ = c.save();

            let mut eng = engine_settings.borrow_mut();
            let mut p = eng.params().clone();
            p.power_limit_enabled = pwr_limit;
            eng.set_params(p);
        },
    );
    let settings_tab_label = gtk::Label::new(Some("Settings"));
    notebook.append_page(&settings_box, Some(&settings_tab_label));

    root_box.append(&notebook);
    window.set_child(Some(&root_box));

    // Power toggle button logic
    let powered_btn_ref = power_btn.clone();
    let strip_state_power = strip_state.clone();
    let ble_handle_power = ble_handle.clone();
    let is_connected_pwr = is_connected.clone();
    let cfg_pwr = config.clone();
    let powered_cell = powered.clone();

    power_btn.connect_clicked(move |_| {
        let new_power = !powered_cell.get();
        powered_cell.set(new_power);
        {
            let mut s = strip_state_power.borrow_mut();
            s.desired_power = new_power;
            s.powered = new_power;
            s.physical_connected = is_connected_pwr.get();
        }

        {
            let mut c = cfg_pwr.borrow_mut();
            c.powered = new_power;
            let _ = c.save();
        }

        if new_power {
            powered_btn_ref.set_label("Power: ON");
            powered_btn_ref.remove_css_class("power-off-btn");
            powered_btn_ref.add_css_class("power-on-btn");
            if is_connected_pwr.get() {
                ble_handle_power.try_send(BleMessage::SendCommand(Command::PowerOn));
            }
        } else {
            powered_btn_ref.set_label("Power: OFF");
            powered_btn_ref.remove_css_class("power-on-btn");
            powered_btn_ref.add_css_class("power-off-btn");
            if is_connected_pwr.get() {
                ble_handle_power.try_send(BleMessage::SendCommand(Command::PowerOff));
            }
        }
    });

    // Handle events from BLE worker on the GTK main loop
    let ble_event_handle = ble_handle.clone();
    let conn_handle_cell = Rc::new(conn_handle);
    let conn_h = conn_handle_cell.clone();
    let is_conn_event = is_connected.clone();
    let strip_state_event = strip_state.clone();
    let event_rx_cell = Rc::new(RefCell::new(event_rx));
    let rx_poll = event_rx_cell.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        while let Ok(event) = rx_poll.borrow().try_recv() {
            match event {
                BleEvent::StateChanged(state) => {
                    match &state {
                        ConnectionState::Ready(name) => {
                            let was_connected = is_conn_event.get();
                            is_conn_event.set(true);
                            {
                                let mut s = strip_state_event.borrow_mut();
                                s.physical_connected = true;
                                s.connection_status = format!("READY: {name}");
                            }
                            if !was_connected {
                                let desired = config.borrow().clone();
                                let power_command = if desired.powered {
                                    Command::PowerOn
                                } else {
                                    Command::PowerOff
                                };
                                ble_event_handle.try_send(BleMessage::SendCommand(power_command));
                                if desired.powered {
                                    if let Some(pattern) = desired.hardware_pattern {
                                        ble_event_handle.try_send(BleMessage::SendCommand(
                                            Command::Brightness(desired.brightness),
                                        ));
                                        ble_event_handle.try_send(BleMessage::SendCommand(
                                            Command::Pattern(pattern),
                                        ));
                                    } else {
                                        ble_event_handle.try_send(BleMessage::SendCommand(
                                            Command::Brightness(100),
                                        ));
                                        ble_event_handle.try_send(BleMessage::SendCommand(
                                            Command::StaticColor {
                                                r: desired.color_r,
                                                g: desired.color_g,
                                                b: desired.color_b,
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                        ConnectionState::SendingCommand(_) => {}
                        ConnectionState::Connecting(name)
                        | ConnectionState::DiscoveringServices(name) => {
                            is_conn_event.set(false);
                            let mut s = strip_state_event.borrow_mut();
                            s.physical_connected = false;
                            s.connection_status = format!("CONNECTING: {name}");
                        }
                        ConnectionState::Reconnecting(name, attempt) => {
                            is_conn_event.set(false);
                            let mut s = strip_state_event.borrow_mut();
                            s.physical_connected = false;
                            s.connection_status =
                                format!("RECONNECTING: {name} (attempt {attempt})");
                        }
                        ConnectionState::Scanning => {
                            is_conn_event.set(false);
                            let mut s = strip_state_event.borrow_mut();
                            s.physical_connected = false;
                            s.connection_status = "SCANNING".into();
                        }
                        ConnectionState::Disconnected => {
                            is_conn_event.set(false);
                            let mut s = strip_state_event.borrow_mut();
                            s.physical_connected = false;
                            s.connection_status = "DISCONNECTED".into();
                        }
                        ConnectionState::Error(message) => {
                            is_conn_event.set(false);
                            let mut s = strip_state_event.borrow_mut();
                            s.physical_connected = false;
                            s.connection_status = format!("BLE ERROR: {message}");
                        }
                    }
                    conn_h.set_state(&state);
                }
                BleEvent::DeviceFound(device) => {
                    config.borrow_mut().last_device_address = Some(device.address.to_string());
                    let _ = config.borrow().save();
                    conn_h.add_device(device);
                }
                BleEvent::ScanComplete => {}
                BleEvent::CommandSent { with_response } => {
                    let mut s = strip_state_event.borrow_mut();
                    s.last_command_status = if with_response {
                        "GATT acknowledged (with response)".to_string()
                    } else {
                        "Sent (without response)".to_string()
                    };
                }
                BleEvent::CommandFailed(err) => {
                    log::error!("Command failed: {err}");
                    let mut s = strip_state_event.borrow_mut();
                    s.last_command_status = format!("Failed: {err}");
                    // ponytail: Do not overwrite s.connection_status; link status is decoupled from command writes
                }
            }
        }
        glib::ControlFlow::Continue
    });

    // Main animation and effect tick loop (~30 fps)
    let engine_tick = engine.clone();
    let music_engine_tick = music_engine.clone();
    let music_active_tick = music_active.clone();
    let strip_tick = strip_state.clone();
    let ble_tick = ble_handle.clone();
    let is_conn_tick = is_connected.clone();
    let mut last_tick = std::time::Instant::now();
    let mut last_send = std::time::Instant::now();
    let mut last_sent_color: Option<(u8, u8, u8)> = None;

    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        let now = std::time::Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64();
        last_tick = now;

        if music_active_tick.get() {
            let (raw_r, raw_g, raw_b) = music_engine_tick.borrow_mut().tick(dt);
            let br = engine_tick.borrow().params().brightness;
            let r = (raw_r as f64 * br).round().clamp(0.0, 255.0) as u8;
            let g = (raw_g as f64 * br).round().clamp(0.0, 255.0) as u8;
            let b = (raw_b as f64 * br).round().clamp(0.0, 255.0) as u8;
            {
                let mut s = strip_tick.borrow_mut();
                s.set_all_leds(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
            }

            // Sync to BLE hardware only if connected (fluid 30 fps streaming)
            if is_conn_tick.get()
                && now.duration_since(last_send) >= std::time::Duration::from_millis(33)
                && last_sent_color != Some((r, g, b))
            {
                last_send = now;
                last_sent_color = Some((r, g, b));
                ble_tick.try_send(BleMessage::SendCommand(Command::StaticColor { r, g, b }));
            }
        } else {
            let mut eng = engine_tick.borrow_mut();
            let current_effect = eng.effect();
            let is_hw_pattern = matches!(current_effect, Effect::HardwarePattern(_));

            if !is_hw_pattern {
                let is_transitioning = eng.is_transitioning();
                if current_effect != Effect::Static || is_transitioning {
                    let (r, g, b) = eng.tick(dt);
                    let leds = eng.compute_strip_leds::<{ ui::SMD_COUNT }>();
                    {
                        let mut s = strip_tick.borrow_mut();
                        s.led_colors = leds;
                        if !is_transitioning && current_effect == Effect::Static {
                            s.effect_active = false;
                        }
                    }

                    // Sync to BLE hardware for software effects and smooth transitions (fluid 30 fps)
                    if is_conn_tick.get()
                        && now.duration_since(last_send) >= std::time::Duration::from_millis(33)
                        && last_sent_color != Some((r, g, b))
                    {
                        last_send = now;
                        last_sent_color = Some((r, g, b));
                        ble_tick.try_send(BleMessage::SendCommand(Command::StaticColor {
                            r,
                            g,
                            b,
                        }));
                    }
                }
            } else {
                let _ = eng.tick(dt);
                let leds = eng.compute_strip_leds::<{ ui::SMD_COUNT }>();
                let mut s = strip_tick.borrow_mut();
                s.led_colors = leds;
            }
        }
        glib::ControlFlow::Continue
    });

    window.present();
}
