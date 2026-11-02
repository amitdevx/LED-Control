// ponytail: Effects engine driving software animations and hardware pattern simulation.
// Includes electrical power budget limiter and soft-start inrush protection.

use palette::{FromColor, Hsl, Srgb};

/// Available built-in effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// No effect, manual colour control.
    Static,
    /// Smooth hue cycling through the full spectrum.
    HueCycle,
    /// Breathing/pulse effect on current colour.
    Breathing,
    /// Strobe/flash effect.
    Strobe,
    /// Colour fade between warm tones.
    WarmFade,
    /// Colour fade between cool tones.
    CoolFade,
    /// Rainbow jump (discrete colour steps).
    RainbowJump,
    /// Candle flicker simulation.
    CandleFlicker,
    /// Cross-fade between two user-selected colours.
    CrossFade,
    /// Built-in hardware pattern index (0-210) running on controller microchip.
    HardwarePattern(u8),
}

impl Effect {
    pub fn label(&self) -> &'static str {
        match self {
            Effect::Static => "Static",
            Effect::HueCycle => "Hue Cycle",
            Effect::Breathing => "Breathing",
            Effect::Strobe => "Strobe",
            Effect::WarmFade => "Warm Fade",
            Effect::CoolFade => "Cool Fade",
            Effect::RainbowJump => "Rainbow Jump",
            Effect::CandleFlicker => "Candle Flicker",
            Effect::CrossFade => "Cross Fade",
            Effect::HardwarePattern(_) => "Hardware Pattern",
        }
    }

    pub fn all_software() -> &'static [Effect] {
        &[
            Effect::Static,
            Effect::HueCycle,
            Effect::Breathing,
            Effect::Strobe,
            Effect::WarmFade,
            Effect::CoolFade,
            Effect::RainbowJump,
            Effect::CandleFlicker,
            Effect::CrossFade,
        ]
    }
}

/// Parameters controlling how effects are rendered.
#[derive(Debug, Clone)]
pub struct EffectParams {
    /// Base colour for effects that modulate a single colour.
    pub base_r: u8,
    pub base_g: u8,
    pub base_b: u8,
    /// Secondary colour for cross-fade.
    pub alt_r: u8,
    pub alt_g: u8,
    pub alt_b: u8,
    /// Speed multiplier (0.1 to 10.0, default 1.0).
    pub speed: f64,
    /// Overall brightness (0.0 to 1.0).
    pub brightness: f64,
    /// Enable electrical current budget limiter (softens 100% white inrush).
    pub power_limit_enabled: bool,
}

impl Default for EffectParams {
    fn default() -> Self {
        Self {
            base_r: 255,
            base_g: 0,
            base_b: 128,
            alt_r: 0,
            alt_g: 128,
            alt_b: 255,
            speed: 1.0,
            brightness: 1.0,
            power_limit_enabled: true,
        }
    }
}

/// Drives software and simulated hardware LED effects with soft-start electrical transitions.
pub struct EffectEngine {
    effect: Effect,
    params: EffectParams,
    /// Internal time accumulator in seconds.
    time: f64,
    /// Smoothed output channels for inrush current protection.
    current_r: f64,
    current_g: f64,
    current_b: f64,
}

impl EffectEngine {
    pub fn new() -> Self {
        Self {
            effect: Effect::Static,
            params: EffectParams::default(),
            time: 0.0,
            current_r: 255.0,
            current_g: 0.0,
            current_b: 128.0,
        }
    }

    pub fn set_effect(&mut self, effect: Effect) {
        self.effect = effect;
        self.time = 0.0;
    }

    pub fn effect(&self) -> Effect {
        self.effect
    }

    pub fn set_params(&mut self, params: EffectParams) {
        self.params = params;
    }

    #[allow(dead_code)]
    pub fn params(&self) -> &EffectParams {
        &self.params
    }

    #[allow(dead_code)]
    pub fn params_mut(&mut self) -> &mut EffectParams {
        &mut self.params
    }

    /// Advance the internal clock by `dt` seconds and return the current RGB output.
    pub fn tick(&mut self, dt: f64) -> (u8, u8, u8) {
        self.time += dt * self.params.speed;

        let (target_r, target_g, target_b) = match self.effect {
            Effect::Static => (self.params.base_r, self.params.base_g, self.params.base_b),
            Effect::HueCycle => self.compute_hue_cycle(),
            Effect::Breathing => self.compute_breathing(),
            Effect::Strobe => self.compute_strobe(),
            Effect::WarmFade => self.compute_warm_fade(),
            Effect::CoolFade => self.compute_cool_fade(),
            Effect::RainbowJump => self.compute_rainbow_jump(),
            Effect::CandleFlicker => self.compute_candle_flicker(),
            Effect::CrossFade => self.compute_cross_fade(),
            Effect::HardwarePattern(idx) => self.compute_hardware_pattern_for_led(idx, 0.0),
        };

        // Apply brightness scaling
        let br = self.params.brightness;
        let mut r = target_r as f64 * br;
        let mut g = target_g as f64 * br;
        let mut b = target_b as f64 * br;

        // Electrical power budget limiter:
        // On 5V USB supplies, full white (R=255, G=255, B=255) draws up to 3A, causing
        // voltage drop and controller brownout. Limit total current sum gracefully.
        if self.params.power_limit_enabled {
            let total = r + g + b;
            let max_safe_sum = 630.0; // ~82% peak power ceiling
            if total > max_safe_sum {
                let factor = max_safe_sum / total;
                r *= factor;
                g *= factor;
                b *= factor;
            }
        }

        // Soft-start slew filter: smooth transition over 60ms to prevent inrush spikes
        let slew_rate = (dt * 18.0).clamp(0.05, 1.0);
        self.current_r += (r - self.current_r) * slew_rate;
        self.current_g += (g - self.current_g) * slew_rate;
        self.current_b += (b - self.current_b) * slew_rate;

        (
            self.current_r.round().clamp(0.0, 255.0) as u8,
            self.current_g.round().clamp(0.0, 255.0) as u8,
            self.current_b.round().clamp(0.0, 255.0) as u8,
        )
    }

    /// Compute per-LED colors across an N-LED strip for authentic wave and chase preview.
    pub fn compute_strip_leds<const N: usize>(&self) -> [(f64, f64, f64); N] {
        let mut output = [(0.0, 0.0, 0.0); N];

        for (i, led) in output.iter_mut().enumerate() {
            let offset = i as f64 / N as f64;
            let (r, g, b) = match self.effect {
                Effect::Static => (self.params.base_r, self.params.base_g, self.params.base_b),
                Effect::HardwarePattern(idx) => self.compute_hardware_pattern_for_led(idx, offset),
                Effect::HueCycle => {
                    // Whole strip walks hue with subtle wave shimmer
                    let (hr, hg, hb) = self.compute_hue_cycle();
                    let wave = ((self.time * 2.0 + offset * std::f64::consts::PI * 2.0).sin()
                        * 0.08
                        + 0.92)
                        .clamp(0.0, 1.0);
                    (
                        ((hr as f64) * wave) as u8,
                        ((hg as f64) * wave) as u8,
                        ((hb as f64) * wave) as u8,
                    )
                }
                Effect::Breathing => self.compute_breathing(),
                Effect::Strobe => self.compute_strobe(),
                Effect::WarmFade => self.compute_warm_fade(),
                Effect::CoolFade => self.compute_cool_fade(),
                Effect::RainbowJump => self.compute_rainbow_jump(),
                Effect::CandleFlicker => self.compute_candle_flicker(),
                Effect::CrossFade => self.compute_cross_fade(),
            };

            *led = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        }

        output
    }

    /// Simulate the controller microchip patterns for accurate visual preview with per-LED offset.
    pub fn compute_hardware_pattern_for_led(&self, idx: u8, offset: f64) -> (u8, u8, u8) {
        match idx {
            // Fade modes (1-8): single-color or spectrum breathing
            1 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                ((255.0 * factor) as u8, 0, 0)
            }
            2 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                (0, (255.0 * factor) as u8, 0)
            }
            3 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                (0, 0, (255.0 * factor) as u8)
            }
            4 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                ((255.0 * factor) as u8, (255.0 * factor) as u8, 0)
            }
            5 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                (0, (255.0 * factor) as u8, (255.0 * factor) as u8)
            }
            6 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                ((255.0 * factor) as u8, 0, (255.0 * factor) as u8)
            }
            7 => {
                let factor = ((self.time * 2.5).sin() + 1.0) / 2.0;
                let v = (255.0 * factor) as u8;
                (v, v, v)
            }
            8 => {
                // 7-Color Fade
                let cycle_secs = 8.0;
                let hue = ((self.time % cycle_secs) / cycle_secs * 360.0) as f32;
                let hsl = Hsl::new(hue, 1.0, 0.5);
                let rgb = Srgb::from_color(hsl);
                (
                    (rgb.red * 255.0) as u8,
                    (rgb.green * 255.0) as u8,
                    (rgb.blue * 255.0) as u8,
                )
            }
            // Jump modes (9-11)
            9 => {
                // 3-Color Jump (Red, Green, Blue)
                let colors = [(255, 0, 0), (0, 255, 0), (0, 0, 255)];
                let step = ((self.time * 2.5) as usize) % colors.len();
                colors[step]
            }
            10 | 11 => {
                // 7-Color Jump
                let colors = [
                    (255, 0, 0),
                    (255, 128, 0),
                    (255, 235, 0),
                    (0, 255, 0),
                    (0, 220, 255),
                    (0, 0, 255),
                    (255, 0, 200),
                ];
                let step = ((self.time * 2.5) as usize) % colors.len();
                colors[step]
            }
            // Strobe modes (12-19)
            12..=19 => {
                let on = ((self.time * 6.0) % 1.0) < 0.45;
                if on {
                    match idx {
                        12 => (255, 0, 0),
                        13 => (0, 255, 0),
                        14 => (0, 0, 255),
                        15 => (255, 255, 0),
                        16 => (0, 255, 255),
                        17 => (255, 0, 255),
                        _ => (255, 255, 255),
                    }
                } else {
                    (0, 0, 0)
                }
            }
            // Rainbow Chase (20-29)
            20..=29 => {
                let hue = ((self.time * 180.0 + offset * 360.0) % 360.0) as f32;
                let hsl = Hsl::new(hue, 1.0, 0.5);
                let rgb = Srgb::from_color(hsl);
                (
                    (rgb.red * 255.0) as u8,
                    (rgb.green * 255.0) as u8,
                    (rgb.blue * 255.0) as u8,
                )
            }
            // Fire Chase (30-39)
            30..=39 => {
                let wave = ((self.time * 3.0 - offset * 4.0) % 1.0 + 1.0) % 1.0;
                let r = (200.0 + wave * 55.0) as u8;
                let g = (wave * 120.0) as u8;
                (r, g, 0)
            }
            // Meteor / Stream (40-49)
            40..=49 => {
                let head = (self.time * 1.5) % 1.0;
                let dist = (head - offset + 1.0) % 1.0;
                if dist < 0.25 {
                    let brightness = 1.0 - (dist / 0.25);
                    (
                        (100.0 * brightness) as u8,
                        (200.0 * brightness) as u8,
                        (255.0 * brightness) as u8,
                    )
                } else {
                    (5, 10, 30)
                }
            }
            // Other controller patterns: flowing spectral cycle
            _ => {
                let hue = ((self.time * 120.0 + offset * 360.0 + idx as f64 * 12.0) % 360.0) as f32;
                let hsl = Hsl::new(hue, 1.0, 0.5);
                let rgb = Srgb::from_color(hsl);
                (
                    (rgb.red * 255.0) as u8,
                    (rgb.green * 255.0) as u8,
                    (rgb.blue * 255.0) as u8,
                )
            }
        }
    }

    /// Smooth hue rotation over a 30-second default cycle.
    fn compute_hue_cycle(&self) -> (u8, u8, u8) {
        let cycle_secs = 30.0;
        let hue = ((self.time % cycle_secs) / cycle_secs * 360.0) as f32;
        let hsl = Hsl::new(hue, 1.0, 0.5);
        let rgb = Srgb::from_color(hsl);
        (
            (rgb.red * 255.0) as u8,
            (rgb.green * 255.0) as u8,
            (rgb.blue * 255.0) as u8,
        )
    }

    /// Pulse the base colour using a sine wave.
    fn compute_breathing(&self) -> (u8, u8, u8) {
        let phase = (self.time * std::f64::consts::TAU / 4.0).sin();
        let factor = (phase + 1.0) / 2.0;
        (
            (self.params.base_r as f64 * factor) as u8,
            (self.params.base_g as f64 * factor) as u8,
            (self.params.base_b as f64 * factor) as u8,
        )
    }

    /// Hard on/off flash at ~4 Hz.
    fn compute_strobe(&self) -> (u8, u8, u8) {
        let on = ((self.time * 4.0) % 1.0) < 0.5;
        if on {
            (self.params.base_r, self.params.base_g, self.params.base_b)
        } else {
            (0, 0, 0)
        }
    }

    /// Fade through warm colours: red -> orange -> yellow -> red.
    fn compute_warm_fade(&self) -> (u8, u8, u8) {
        let cycle_secs = 8.0;
        let hue = ((self.time % cycle_secs) / cycle_secs * 60.0) as f32;
        let hsl = Hsl::new(hue, 1.0, 0.5);
        let rgb = Srgb::from_color(hsl);
        (
            (rgb.red * 255.0) as u8,
            (rgb.green * 255.0) as u8,
            (rgb.blue * 255.0) as u8,
        )
    }

    /// Fade through cool colours: blue -> cyan -> violet -> blue.
    fn compute_cool_fade(&self) -> (u8, u8, u8) {
        let cycle_secs = 8.0;
        let t = (self.time % cycle_secs) / cycle_secs;
        let hue = (200.0 + t * 120.0) as f32;
        let hsl = Hsl::new(hue % 360.0, 1.0, 0.5);
        let rgb = Srgb::from_color(hsl);
        (
            (rgb.red * 255.0) as u8,
            (rgb.green * 255.0) as u8,
            (rgb.blue * 255.0) as u8,
        )
    }

    /// Jump between 7 rainbow colours every ~0.5 seconds.
    fn compute_rainbow_jump(&self) -> (u8, u8, u8) {
        let rainbow_hues: [f32; 7] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 300.0];
        let step = (self.time * 2.0) as usize % rainbow_hues.len();
        let hsl = Hsl::new(rainbow_hues[step], 1.0, 0.5);
        let rgb = Srgb::from_color(hsl);
        (
            (rgb.red * 255.0) as u8,
            (rgb.green * 255.0) as u8,
            (rgb.blue * 255.0) as u8,
        )
    }

    /// Candle flicker effect: warm amber with subtle random-walk variations.
    fn compute_candle_flicker(&self) -> (u8, u8, u8) {
        let f1 = (self.time * 7.3).sin() * 0.15;
        let f2 = (self.time * 13.7).sin() * 0.10;
        let f3 = (self.time * 23.1).sin() * 0.05;
        let factor = (0.70 + f1 + f2 + f3).clamp(0.0, 1.0);

        let r = (255.0 * factor) as u8;
        let g = (140.0 * factor * 0.85) as u8;
        let b = (20.0 * factor * 0.4) as u8;
        (r, g, b)
    }

    /// Cross-fade between base colour and alt colour.
    fn compute_cross_fade(&self) -> (u8, u8, u8) {
        let cycle_secs = 6.0;
        let t = ((self.time % cycle_secs) / cycle_secs * std::f64::consts::TAU).sin();
        let factor = (t + 1.0) / 2.0;

        let r =
            (self.params.base_r as f64 * (1.0 - factor) + self.params.alt_r as f64 * factor) as u8;
        let g =
            (self.params.base_g as f64 * (1.0 - factor) + self.params.alt_g as f64 * factor) as u8;
        let b =
            (self.params.base_b as f64 * (1.0 - factor) + self.params.alt_b as f64 * factor) as u8;
        (r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_tick() {
        let mut engine = EffectEngine::new();
        let (r, _g, _b) = engine.tick(0.016);
        assert!(r > 0);
    }

    #[test]
    fn test_power_budget_limit() {
        let mut engine = EffectEngine::new();
        engine.set_params(EffectParams {
            base_r: 255,
            base_g: 255,
            base_b: 255,
            power_limit_enabled: true,
            ..Default::default()
        });
        // Run several ticks for slew filter to stabilize
        let mut out = (0, 0, 0);
        for _ in 0..60 {
            out = engine.tick(0.016);
        }
        let sum = out.0 as u32 + out.1 as u32 + out.2 as u32;
        assert!(sum <= 640);
    }

    #[test]
    fn test_hardware_pattern_simulation() {
        let mut engine = EffectEngine::new();
        engine.set_effect(Effect::HardwarePattern(1)); // Red Fade
        let (_r, g, b) = engine.tick(0.1);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }
}
