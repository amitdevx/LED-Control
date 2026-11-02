// ponytail: LED strip visualization widget for desktop simulation.
// Note: The physical One94Store 5m strip operates as a uniform whole-strip bus.
// This widget renders 28 preview segments on screen as a desktop visual simulator,
// not a representation of individually addressable physical hardware LEDs.

use gtk::prelude::*;
use gtk::{glib, DrawingArea};
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

pub const STRIP_HEIGHT: i32 = 78;
pub const DEFAULT_PREVIEW_SEGMENTS: usize = 28;
pub const SMD_COUNT: usize = DEFAULT_PREVIEW_SEGMENTS;
const SMD_SIZE: f64 = 16.0;
const CUP_RADIUS: f64 = 5.8;

/// State for the animated LED strip.
#[derive(Debug, Clone)]
pub struct StripState {
    /// Master RGB fallback color (0.0 to 1.0).
    pub r: f64,
    pub g: f64,
    pub b: f64,
    /// Desired power state set by the user.
    pub desired_power: bool,
    /// Physical link state to the hardware controller.
    pub physical_connected: bool,
    /// Last acknowledged command status description.
    pub last_command_status: String,
    /// Simulator active state (whether local canvas preview is active).
    #[allow(dead_code)]
    pub simulator_active: bool,
    /// Backward-compatible alias for desired_power.
    pub powered: bool,
    /// Master brightness (0.0 to 1.0).
    pub brightness: f64,
    /// Animation phase for dynamic light wave.
    pub phase: f64,
    /// Active dynamic effect.
    pub effect_active: bool,
    /// Connection indicator text.
    pub connection_status: String,
    /// Per-segment visual preview color buffer for the GTK canvas.
    pub led_colors: [(f64, f64, f64); SMD_COUNT],
}

impl Default for StripState {
    fn default() -> Self {
        let initial_color = (1.0, 0.0, 0.5);
        Self {
            r: initial_color.0,
            g: initial_color.1,
            b: initial_color.2,
            desired_power: false,
            physical_connected: false,
            last_command_status: "None".into(),
            simulator_active: true,
            powered: false, // Default to unpowered until explicit user activation or config load
            brightness: 1.0,
            phase: 0.0,
            effect_active: false,
            connection_status: "Disconnected".into(),
            led_colors: [(0.0, 0.0, 0.0); SMD_COUNT],
        }
    }
}

impl StripState {
    pub fn set_all_leds(&mut self, r: f64, g: f64, b: f64) {
        self.r = r;
        self.g = g;
        self.b = b;
        for led in &mut self.led_colors {
            *led = (r, g, b);
        }
    }
}

/// Create the LED strip visualization widget with its animation state.
pub fn create_strip_widget() -> (DrawingArea, Rc<RefCell<StripState>>) {
    let state = Rc::new(RefCell::new(StripState::default()));
    let drawing_area = DrawingArea::new();
    drawing_area.set_content_height(STRIP_HEIGHT);
    drawing_area.set_hexpand(true);
    drawing_area.set_vexpand(false);
    drawing_area.add_css_class("led-strip");

    let state_ref = state.clone();
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let s = state_ref.borrow();
        draw_smd_strip(cr, width as f64, height as f64, &s);
    });

    // Smooth 30 fps render timer
    let state_anim = state.clone();
    let area_ref = drawing_area.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        {
            let mut s = state_anim.borrow_mut();
            s.phase += 0.05;
            if s.phase > PI * 2.0 {
                s.phase -= PI * 2.0;
            }
        }
        area_ref.queue_draw();
        glib::ControlFlow::Continue
    });

    (drawing_area, state)
}

/// Draw the authentic SMD 5050 LED strip.
fn draw_smd_strip(cr: &cairo::Context, width: f64, height: f64, state: &StripState) {
    let ribbon_height = 38.0;
    let ribbon_y = (height - ribbon_height) / 2.0;
    let ribbon_margin = 12.0;
    let ribbon_width = (width - ribbon_margin * 2.0).max(100.0);

    let br = if state.powered { state.brightness } else { 0.0 };

    // 1. Ambient wall reflection wash behind the strip
    if state.powered && br > 0.02 {
        // Average color across LEDs for ambient background wash
        let mut avg_r = 0.0;
        let mut avg_g = 0.0;
        let mut avg_b = 0.0;
        for (lr, lg, lb) in &state.led_colors {
            avg_r += lr;
            avg_g += lg;
            avg_b += lb;
        }
        avg_r = (avg_r / SMD_COUNT as f64) * br;
        avg_g = (avg_g / SMD_COUNT as f64) * br;
        avg_b = (avg_b / SMD_COUNT as f64) * br;

        let wash = cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
        wash.add_color_stop_rgba(0.0, avg_r, avg_g, avg_b, 0.22 * br);
        wash.add_color_stop_rgba(0.5, avg_r, avg_g, avg_b, 0.06 * br);
        wash.add_color_stop_rgba(1.0, avg_r, avg_g, avg_b, 0.22 * br);
        cr.rectangle(0.0, 0.0, width, height);
        cr.set_source(&wash).unwrap();
        let _ = cr.fill();
    }

    // 2. Flexible PCB Tape Ribbon substrate (Dark matte with subtle metallic sheen)
    rounded_rect(
        cr,
        ribbon_margin,
        ribbon_y,
        ribbon_width,
        ribbon_height,
        6.0,
    );
    cr.set_source_rgb(0.08, 0.09, 0.12);
    let _ = cr.fill();

    // Subtle edge bevel
    rounded_rect(
        cr,
        ribbon_margin,
        ribbon_y,
        ribbon_width,
        ribbon_height,
        6.0,
    );
    cr.set_source_rgba(0.24, 0.27, 0.35, 0.7);
    cr.set_line_width(1.0);
    let _ = cr.stroke();

    // 3. Continuous copper power rails (+5V and GND traces along top and bottom)
    let rail_top_y = ribbon_y + 4.5;
    let rail_bot_y = ribbon_y + ribbon_height - 4.5;
    cr.set_source_rgba(0.72, 0.48, 0.22, 0.55); // Copper tint
    cr.set_line_width(1.2);
    cr.move_to(ribbon_margin + 6.0, rail_top_y);
    cr.line_to(ribbon_margin + ribbon_width - 6.0, rail_top_y);
    let _ = cr.stroke();

    cr.move_to(ribbon_margin + 6.0, rail_bot_y);
    cr.line_to(ribbon_margin + ribbon_width - 6.0, rail_bot_y);
    let _ = cr.stroke();

    // 4. Calculate layout spacing for SMD 5050 packages
    let available_width = ribbon_width - 32.0;
    let step = available_width / (SMD_COUNT - 1) as f64;
    let cy = ribbon_y + ribbon_height / 2.0;

    for i in 0..SMD_COUNT {
        let cx = ribbon_margin + 16.0 + i as f64 * step;

        // Cut line and copper solder pads every 4 LEDs
        if i > 0 && i % 4 == 0 {
            let cut_x = cx - step / 2.0;
            // 4 mini copper pads (+5V, R, G, B)
            cr.set_source_rgba(0.85, 0.62, 0.25, 0.8);
            for p in 0..4 {
                let pad_y = ribbon_y + 7.0 + p as f64 * 6.5;
                cr.rectangle(cut_x - 3.0, pad_y, 6.0, 3.5);
                let _ = cr.fill();
            }
            // Cut dotted line
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.5);
            cr.set_line_width(0.8);
            cr.move_to(cut_x, ribbon_y + 2.0);
            cr.line_to(cut_x, ribbon_y + ribbon_height - 2.0);
            let _ = cr.stroke();
        }

        // SMD Resistor between LEDs
        if i % 2 == 1 {
            let res_x = cx - step / 2.0;
            let res_y = cy - 2.5;
            // Black ceramic body
            cr.set_source_rgb(0.04, 0.04, 0.05);
            cr.rectangle(res_x - 3.5, res_y, 7.0, 5.0);
            let _ = cr.fill();
            // Silver solder endcaps
            cr.set_source_rgb(0.65, 0.68, 0.72);
            cr.rectangle(res_x - 3.5, res_y, 1.5, 5.0);
            let _ = cr.fill();
            cr.rectangle(res_x + 2.0, res_y, 1.5, 5.0);
            let _ = cr.fill();
        }

        // Per-LED color from buffer
        let (base_r, base_g, base_b) = state.led_colors[i];
        let led_r = (base_r * br).clamp(0.0, 1.0);
        let led_g = (base_g * br).clamp(0.0, 1.0);
        let led_b = (base_b * br).clamp(0.0, 1.0);

        // 5. Intense radial glow from diode when powered
        if state.powered && br > 0.02 {
            let glow_radius = 22.0 * br;
            let glow = cairo::RadialGradient::new(cx, cy, CUP_RADIUS * 0.5, cx, cy, glow_radius);
            glow.add_color_stop_rgba(0.0, led_r, led_g, led_b, 0.80 * br);
            glow.add_color_stop_rgba(0.35, led_r, led_g, led_b, 0.30 * br);
            glow.add_color_stop_rgba(1.0, led_r, led_g, led_b, 0.0);
            cr.arc(cx, cy, glow_radius, 0.0, PI * 2.0);
            cr.set_source(&glow).unwrap();
            let _ = cr.fill();
        }

        // 6. SMD 5050 White Ceramic Package (Square 16x16 with rounded corners)
        let smd_x = cx - SMD_SIZE / 2.0;
        let smd_y = cy - SMD_SIZE / 2.0;
        rounded_rect(cr, smd_x, smd_y, SMD_SIZE, SMD_SIZE, 2.5);
        cr.set_source_rgb(0.88, 0.89, 0.92);
        let _ = cr.fill();

        // 6 metal side leads (3 on top, 3 on bottom)
        cr.set_source_rgb(0.70, 0.72, 0.76);
        for pin in 0..3 {
            let px = smd_x + 2.5 + pin as f64 * 4.5;
            cr.rectangle(px, smd_y - 1.2, 2.0, 1.2);
            let _ = cr.fill();
            cr.rectangle(px, smd_y + SMD_SIZE, 2.0, 1.2);
            let _ = cr.fill();
        }

        // Pin 1 notch marker
        cr.set_source_rgb(0.72, 0.73, 0.77);
        cr.rectangle(smd_x + 1.0, smd_y + 1.0, 2.5, 2.5);
        let _ = cr.fill();

        // 7. Circular Recessed Reflector Cup
        cr.arc(cx, cy, CUP_RADIUS, 0.0, PI * 2.0);
        if state.powered && br > 0.02 {
            // Hot emitter core
            let emitter = cairo::RadialGradient::new(cx, cy, 1.0, cx, cy, CUP_RADIUS);
            emitter.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.95); // White-hot core
            emitter.add_color_stop_rgba(0.28, led_r, led_g, led_b, 1.0);
            emitter.add_color_stop_rgba(1.0, led_r * 0.75, led_g * 0.75, led_b * 0.75, 1.0);
            cr.set_source(&emitter).unwrap();
        } else {
            // Unlit silicone phosphor cup
            cr.set_source_rgb(0.35, 0.32, 0.25);
        }
        let _ = cr.fill();

        // Reflector rim ring
        cr.arc(cx, cy, CUP_RADIUS, 0.0, PI * 2.0);
        cr.set_source_rgba(0.65, 0.67, 0.72, 0.7);
        cr.set_line_width(0.7);
        let _ = cr.stroke();

        // 8. 3 RGB Semiconductor Dies visible inside cup when unlit
        if !state.powered || br <= 0.05 {
            cr.set_source_rgb(0.20, 0.18, 0.15);
            cr.rectangle(cx - 2.8, cy - 1.2, 1.4, 2.4);
            cr.rectangle(cx - 0.7, cy - 1.2, 1.4, 2.4);
            cr.rectangle(cx + 1.4, cy - 1.2, 1.4, 2.4);
            let _ = cr.fill();
        }

        // 9. Dome Specular Glint Highlight
        let glint = cairo::RadialGradient::new(
            cx - CUP_RADIUS * 0.35,
            cy - CUP_RADIUS * 0.35,
            0.2,
            cx,
            cy,
            CUP_RADIUS,
        );
        glint.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, if state.powered { 0.55 } else { 0.25 });
        glint.add_color_stop_rgba(0.6, 1.0, 1.0, 1.0, 0.0);
        cr.arc(cx, cy, CUP_RADIUS, 0.0, PI * 2.0);
        cr.set_source(&glint).unwrap();
        let _ = cr.fill();
    }

    // Status indicator badge in top left corner
    let is_pwr = state.desired_power || state.powered;
    let dot_color = if state.physical_connected && is_pwr {
        (0.06, 0.85, 0.45) // Active bright green (hardware link connected and powered)
    } else if is_pwr {
        (0.20, 0.60, 0.95) // Active blue (local desktop simulator preview only)
    } else {
        (0.65, 0.45, 0.15) // Standby amber (powered off)
    };

    cr.arc(ribbon_margin + 8.0, 10.0, 3.5, 0.0, PI * 2.0);
    cr.set_source_rgb(dot_color.0, dot_color.1, dot_color.2);
    let _ = cr.fill();

    cr.set_source_rgba(0.70, 0.78, 0.90, 0.9);
    cr.select_font_face(
        "sans-serif",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(9.5);
    cr.move_to(ribbon_margin + 16.0, 13.0);
    let hw_status = if state.physical_connected {
        "HARDWARE UNVERIFIED"
    } else {
        "LOCAL PREVIEW ONLY"
    };
    let pwr_text = if is_pwr { "POWER ON" } else { "STANDBY" };
    let status_str = if state.last_command_status != "None" {
        format!(
            "{} | {} | {} | CMD: {}",
            state.connection_status, pwr_text, hw_status, state.last_command_status
        )
    } else {
        format!("{} | {} | {}", state.connection_status, pwr_text, hw_status)
    };
    let _ = cr.show_text(&status_str);
}

/// Helper function to draw a path with rounded rectangle corners.
fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}
