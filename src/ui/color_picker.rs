// ponytail: Colour picker with custom Cairo solid color swatches, RGB channel sliders, and hex input.
// Replaces fragile GTK CSS button hacks with direct Cairo vector rendering.

use gtk::prelude::*;
use gtk::{Box, DrawingArea, Entry, Label, Orientation, Scale};
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

/// Current colour state shared between picker components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorState {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub h: f64, // 0-360
    pub s: f64, // 0-1
    pub v: f64, // 0-1
}

impl Default for ColorState {
    fn default() -> Self {
        Self {
            r: 255,
            g: 0,
            b: 128,
            h: 330.0,
            s: 1.0,
            v: 1.0,
        }
    }
}

impl ColorState {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        let (h, s, v) = rgb_to_hsv(r, g, b);
        Self { r, g, b, h, s, v }
    }

    #[allow(dead_code)]
    pub fn from_hsv(h: f64, s: f64, v: f64) -> Self {
        let (r, g, b) = hsv_to_rgb(h, s, v);
        Self { r, g, b, h, s, v }
    }

    pub fn hex_string(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

pub struct ColorPickerHandle {
    #[allow(dead_code)]
    pub state: Rc<Cell<ColorState>>,
    pub sync_ui: Rc<dyn Fn(u8, u8, u8)>,
}

/// Builds the colour picker panel and returns the container and handle.
pub fn create_color_picker(
    on_change: impl Fn(u8, u8, u8) + Clone + 'static,
) -> (Box, ColorPickerHandle) {
    let state = Rc::new(Cell::new(ColorState::default()));
    let container = Box::new(Orientation::Vertical, 8);
    container.add_css_class("color-picker-panel");

    // Guard flag to prevent recursive updates when sliders are set programmatically
    let updating = Rc::new(Cell::new(false));

    // Section: Preset colours
    let preset_label = Label::new(Some("Quick Color Presets"));
    preset_label.add_css_class("section-label");
    preset_label.set_halign(gtk::Align::Start);
    container.append(&preset_label);

    let presets_flow = gtk::FlowBox::new();
    presets_flow.set_max_children_per_line(13);
    presets_flow.set_min_children_per_line(6);
    presets_flow.set_selection_mode(gtk::SelectionMode::None);
    presets_flow.set_halign(gtk::Align::Center);
    presets_flow.set_margin_bottom(8);
    presets_flow.set_column_spacing(6);
    presets_flow.set_row_spacing(6);

    let preset_colors: Vec<(u8, u8, u8, &str)> = vec![
        (255, 0, 0, "Pure Red"),
        (255, 75, 0, "Vibrant Orange"),
        (255, 160, 0, "Warm Amber"),
        (255, 230, 0, "Bright Yellow"),
        (100, 255, 0, "Lime Green"),
        (0, 255, 0, "Pure Green"),
        (0, 255, 160, "Turquoise"),
        (0, 220, 255, "Cyan"),
        (0, 100, 255, "Sky Blue"),
        (0, 0, 255, "Deep Blue"),
        (140, 0, 255, "Purple"),
        (255, 0, 180, "Magenta"),
        (255, 255, 255, "Pure White"),
    ];

    // Sliders
    let (r_row, r_scale, r_val_lbl) = create_channel_slider("R", Channel::Red);
    let (g_row, g_scale, g_val_lbl) = create_channel_slider("G", Channel::Green);
    let (b_row, b_scale, b_val_lbl) = create_channel_slider("B", Channel::Blue);

    // Hex input row with live preview chip
    let hex_row = Box::new(Orientation::Horizontal, 8);
    hex_row.set_margin_top(6);
    let hex_label = Label::new(Some("Hex Color"));
    hex_label.add_css_class("section-label");
    hex_label.set_width_chars(10);
    let hex_entry = Entry::new();
    hex_entry.set_max_length(7);
    hex_entry.set_placeholder_text(Some("#FF0080"));
    hex_entry.set_hexpand(true);
    hex_entry.add_css_class("hex-entry");
    hex_entry.set_text(&state.get().hex_string());

    let initial = state.get();
    r_scale.set_value(initial.r as f64);
    r_val_lbl.set_text(&initial.r.to_string());
    g_scale.set_value(initial.g as f64);
    g_val_lbl.set_text(&initial.g.to_string());
    b_scale.set_value(initial.b as f64);
    b_val_lbl.set_text(&initial.b.to_string());

    let swatch_widgets: Rc<RefCell<Vec<DrawingArea>>> = Rc::new(RefCell::new(Vec::new()));

    // Helper to sync sliders, hex entry, and redraw swatches
    let sync_ui = {
        let updating = updating.clone();
        let r_scale = r_scale.clone();
        let g_scale = g_scale.clone();
        let b_scale = b_scale.clone();
        let r_val_lbl = r_val_lbl.clone();
        let g_val_lbl = g_val_lbl.clone();
        let b_val_lbl = b_val_lbl.clone();
        let hex_entry = hex_entry.clone();
        let swatches = swatch_widgets.clone();

        Rc::new(move |r: u8, g: u8, b: u8| {
            updating.set(true);
            r_scale.set_value(r as f64);
            r_val_lbl.set_text(&r.to_string());
            g_scale.set_value(g as f64);
            g_val_lbl.set_text(&g.to_string());
            b_scale.set_value(b as f64);
            b_val_lbl.set_text(&b.to_string());
            hex_entry.set_text(&format!("#{:02X}{:02X}{:02X}", r, g, b));
            updating.set(false);

            for swatch in swatches.borrow().iter() {
                swatch.queue_draw();
            }
        })
    };

    let on_change_rc = Rc::new(on_change.clone());

    // Build custom Cairo circular swatch balls
    for (r, g, b, name) in &preset_colors {
        let swatch = create_color_swatch(
            *r,
            *g,
            *b,
            name,
            state.clone(),
            sync_ui.clone(),
            on_change_rc.clone(),
        );
        swatch_widgets.borrow_mut().push(swatch.clone());
        presets_flow.insert(&swatch, -1);
    }
    container.append(&presets_flow);

    // Section: RGB Sliders
    let rgb_label = Label::new(Some("RGB Channels"));
    rgb_label.add_css_class("section-label");
    rgb_label.set_halign(gtk::Align::Start);
    rgb_label.set_margin_top(4);
    container.append(&rgb_label);

    // Connect slider listeners
    setup_slider_listener(
        &r_scale,
        &r_val_lbl,
        Channel::Red,
        state.clone(),
        updating.clone(),
        hex_entry.clone(),
        swatch_widgets.clone(),
        on_change.clone(),
    );
    setup_slider_listener(
        &g_scale,
        &g_val_lbl,
        Channel::Green,
        state.clone(),
        updating.clone(),
        hex_entry.clone(),
        swatch_widgets.clone(),
        on_change.clone(),
    );
    setup_slider_listener(
        &b_scale,
        &b_val_lbl,
        Channel::Blue,
        state.clone(),
        updating.clone(),
        hex_entry.clone(),
        swatch_widgets.clone(),
        on_change.clone(),
    );

    container.append(&r_row);
    container.append(&g_row);
    container.append(&b_row);

    // Hex activation listener
    let state_hex = state.clone();
    let on_change_hex = on_change.clone();
    let sync_ui_hex = sync_ui.clone();
    hex_entry.connect_activate(move |entry| {
        let text = entry.text().to_string();
        if let Some((r, g, b)) = parse_hex_color(&text) {
            let new_state = ColorState::from_rgb(r, g, b);
            state_hex.set(new_state);
            sync_ui_hex(r, g, b);
            on_change_hex(r, g, b);
        }
    });

    hex_row.append(&hex_label);
    hex_row.append(&hex_entry);
    container.append(&hex_row);

    let handle = ColorPickerHandle { state, sync_ui };

    (container, handle)
}

/// Create a dedicated Cairo DrawingArea swatch ball.
/// Completely eliminates GTK CSS button theme parser bugs and center shadow spots.
fn create_color_swatch(
    r: u8,
    g: u8,
    b: u8,
    tooltip: &str,
    state: Rc<Cell<ColorState>>,
    sync_ui: Rc<dyn Fn(u8, u8, u8)>,
    on_change: Rc<dyn Fn(u8, u8, u8)>,
) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_content_width(34);
    area.set_content_height(34);
    area.set_tooltip_text(Some(tooltip));
    area.set_cursor_from_name(Some("pointer"));

    let hovered = Rc::new(Cell::new(false));
    let hovered_enter = hovered.clone();
    let area_enter = area.clone();
    let motion = gtk::EventControllerMotion::new();
    motion.connect_enter(move |_, _, _| {
        hovered_enter.set(true);
        area_enter.queue_draw();
    });
    let hovered_leave = hovered.clone();
    let area_leave = area.clone();
    motion.connect_leave(move |_| {
        hovered_leave.set(false);
        area_leave.queue_draw();
    });
    area.add_controller(motion);

    let state_click = state.clone();
    let sync_click = sync_ui.clone();
    let change_click = on_change.clone();
    let gesture = gtk::GestureClick::new();
    gesture.connect_pressed(move |_, _, _, _| {
        let new_state = ColorState::from_rgb(r, g, b);
        state_click.set(new_state);
        sync_click(r, g, b);
        change_click(r, g, b);
    });
    area.add_controller(gesture);

    let state_draw = state.clone();
    let hovered_draw = hovered.clone();
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);

    area.set_draw_func(move |_, cr, width, height| {
        let cx = width as f64 / 2.0;
        let cy = height as f64 / 2.0;
        let radius = (cx.min(cy) - 3.0).max(1.0);
        let cur = state_draw.get();
        let is_selected = cur.r == r && cur.g == g && cur.b == b;
        let is_hovered = hovered_draw.get();

        // Subtle bloom behind swatch on hover or active selection
        if is_selected || is_hovered {
            let glow_r = radius + if is_selected { 2.5 } else { 1.5 };
            cr.arc(cx, cy, glow_r, 0.0, 2.0 * PI);
            cr.set_source_rgba(rf, gf, bf, if is_selected { 0.45 } else { 0.25 });
            let _ = cr.fill();
        }

        // Pure solid vibrant color circle
        cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
        cr.set_source_rgb(rf, gf, bf);
        let _ = cr.fill_preserve();

        // Border ring
        if is_selected {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
            cr.set_line_width(2.2);
        } else if is_hovered {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.65);
            cr.set_line_width(1.8);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
            cr.set_line_width(1.0);
        }
        let _ = cr.stroke();

        // High contrast selection indicator pip
        if is_selected {
            cr.arc(cx, cy, radius * 0.32, 0.0, 2.0 * PI);
            let luminance = 0.299 * rf + 0.587 * gf + 0.114 * bf;
            if luminance > 0.65 {
                cr.set_source_rgba(0.08, 0.08, 0.10, 0.85);
            } else {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
            }
            let _ = cr.fill();
        }
    });

    area
}

#[derive(Clone, Copy)]
enum Channel {
    Red,
    Green,
    Blue,
}

fn create_channel_slider(label: &str, channel: Channel) -> (Box, Scale, Label) {
    let row = Box::new(Orientation::Horizontal, 8);
    let lbl = Label::new(Some(label));
    lbl.set_width_chars(2);
    lbl.add_css_class("channel-label");

    let channel_class = match channel {
        Channel::Red => "slider-red",
        Channel::Green => "slider-green",
        Channel::Blue => "slider-blue",
    };

    let scale = Scale::with_range(Orientation::Horizontal, 0.0, 255.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(false);
    scale.add_css_class(channel_class);

    let val_lbl = Label::new(Some("0"));
    val_lbl.set_width_chars(4);
    val_lbl.add_css_class("value-label");

    row.append(&lbl);
    row.append(&scale);
    row.append(&val_lbl);

    (row, scale, val_lbl)
}

#[allow(clippy::too_many_arguments)]
fn setup_slider_listener(
    scale: &Scale,
    val_lbl: &Label,
    channel: Channel,
    state: Rc<Cell<ColorState>>,
    updating: Rc<Cell<bool>>,
    hex_entry: Entry,
    swatches: Rc<RefCell<Vec<DrawingArea>>>,
    on_change: impl Fn(u8, u8, u8) + Clone + 'static,
) {
    let lbl = val_lbl.clone();
    scale.connect_value_changed(move |s| {
        if updating.get() {
            return;
        }

        let val = s.value().round() as u8;
        lbl.set_text(&val.to_string());

        let cur = state.get();
        let (r, g, b) = match channel {
            Channel::Red => (val, cur.g, cur.b),
            Channel::Green => (cur.r, val, cur.b),
            Channel::Blue => (cur.r, cur.g, val),
        };

        state.set(ColorState::from_rgb(r, g, b));

        updating.set(true);
        hex_entry.set_text(&format!("#{:02X}{:02X}{:02X}", r, g, b));
        updating.set(false);

        for swatch in swatches.borrow().iter() {
            swatch.queue_draw();
        }

        on_change(r, g, b);
    });
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let v = max;

    (h, s, v)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}
