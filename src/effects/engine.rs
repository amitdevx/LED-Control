// ponytail: Effects engine with explicit capability separation between whole-strip
// BLE transmission, controller-internal patterns, and local preview simulation.
// Strictly zero emojis and zero em-dashes.

use palette::{FromColor, Hsl, Srgb};

/// The transmission capability of an effect mode.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectCapability {
    /// Transmitted as whole-strip 9-byte static color frames over BLE (0x7B FF 07 ...).
    WholeStripColor,
    /// Activated once via 9-byte pattern command (0x7B FF 03 idx ...); executed by the controller microchip.
    HardwareControllerPattern,
    /// Local desktop visual preview only; not transmitted over the physical BLE link.
    LocalPreviewOnly,
}

/// Available effect modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Static single whole-strip color.
    Static,
    /// Continuous whole-strip spectrum walk.
    HueCycle,
    /// Whole-strip breathing pulse on base color.
    Breathing,
    /// Whole-strip strobe flash.
    Strobe,
    /// Whole-strip warm tone fade.
    WarmFade,
    /// Whole-strip cool tone fade.
    CoolFade,
    /// Whole-strip discrete rainbow jump.
    RainbowJump,
    /// Whole-strip candle flicker simulation.
    CandleFlicker,
    /// Whole-strip cross-fade between two colors.
    CrossFade,
    /// Built-in controller microchip pattern routine (index 0 to 210).
    HardwarePattern(u8),
}

impl Effect {
    pub fn label(&self) -> &'static str {
        match self {
            Effect::Static => "Static Color",
            Effect::HueCycle => "Hue Cycle (Whole Strip)",
            Effect::Breathing => "Breathing (Whole Strip)",
            Effect::Strobe => "Strobe (Whole Strip)",
            Effect::WarmFade => "Warm Fade (Whole Strip)",
            Effect::CoolFade => "Cool Fade (Whole Strip)",
            Effect::RainbowJump => "Rainbow Jump (Whole Strip)",
            Effect::CandleFlicker => "Candle Flicker (Whole Strip)",
            Effect::CrossFade => "Cross Fade (Whole Strip)",
            Effect::HardwarePattern(_) => "Hardware Pattern (Microchip Routine)",
        }
    }

    #[allow(dead_code)]
    pub fn capability(&self) -> EffectCapability {
        match self {
            Effect::Static
            | Effect::HueCycle
            | Effect::Breathing
            | Effect::Strobe
            | Effect::WarmFade
            | Effect::CoolFade
            | Effect::RainbowJump
            | Effect::CandleFlicker
            | Effect::CrossFade => EffectCapability::WholeStripColor,
            Effect::HardwarePattern(_) => EffectCapability::HardwareControllerPattern,
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

/// Parameters controlling effect generation.
#[derive(Debug, Clone)]
pub struct EffectParams {
    pub base_r: u8,
    pub base_g: u8,
    pub base_b: u8,
    pub alt_r: u8,
    pub alt_g: u8,
    pub alt_b: u8,
    pub speed: f64,
    pub brightness: f64,
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

/// Engine generating whole-strip colors for physical transmission and preview segments for the GUI.
pub struct EffectEngine {
    effect: Effect,
    params: EffectParams,
    time: f64,
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

    pub fn params(&self) -> &EffectParams {
        &self.params
    }

    /// Advance time and compute the physical whole-strip output.
    /// Returns the exact (r, g, b) values to be encoded and transmitted over BLE.
    pub fn tick_whole_strip(&mut self, dt: f64) -> (u8, u8, u8) {
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
            Effect::HardwarePattern(idx) => self.simulate_hardware_pattern(idx),
        };

        let br = self.params.brightness;
        let mut r = target_r as f64 * br;
        let mut g = target_g as f64 * br;
        let mut b = target_b as f64 * br;

        // Software power budget limiter for local color synthesis
        if self.params.power_limit_enabled {
            let total = r + g + b;
            let max_safe = 630.0;
            if total > max_safe {
                let factor = max_safe / total;
                r *= factor;
                g *= factor;
                b *= factor;
            }
        }

        // Soft-start slew rate filter: prevents sudden inrush spikes
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

    /// Backward-compatible alias for tick_whole_strip.
    pub fn tick(&mut self, dt: f64) -> (u8, u8, u8) {
        self.tick_whole_strip(dt)
    }

    /// Compute visual preview segments for the local GTK canvas.
    /// Whole-strip effects display uniform segments matching physical output.
    pub fn compute_preview_segments<const N: usize>(&self) -> [(f64, f64, f64); N] {
        let mut output = [(0.0, 0.0, 0.0); N];
        let (wr, wg, wb) = match self.effect {
            Effect::Static => (self.params.base_r, self.params.base_g, self.params.base_b),
            Effect::HueCycle => self.compute_hue_cycle(),
            Effect::Breathing => self.compute_breathing(),
            Effect::Strobe => self.compute_strobe(),
            Effect::WarmFade => self.compute_warm_fade(),
            Effect::CoolFade => self.compute_cool_fade(),
            Effect::RainbowJump => self.compute_rainbow_jump(),
            Effect::CandleFlicker => self.compute_candle_flicker(),
            Effect::CrossFade => self.compute_cross_fade(),
            Effect::HardwarePattern(idx) => self.simulate_hardware_pattern(idx),
        };

        let norm_color = (wr as f64 / 255.0, wg as f64 / 255.0, wb as f64 / 255.0);
        output.fill(norm_color);
        output
    }

    /// Alias for compute_preview_segments.
    pub fn compute_strip_leds<const N: usize>(&self) -> [(f64, f64, f64); N] {
        self.compute_preview_segments::<N>()
    }

    /// Approximate visual simulation for controller-internal pattern preview in the GUI.
    fn simulate_hardware_pattern(&self, idx: u8) -> (u8, u8, u8) {
        match idx {
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
            9 => {
                let colors = [(255, 0, 0), (0, 255, 0), (0, 0, 255)];
                let step = ((self.time * 2.5) as usize) % colors.len();
                colors[step]
            }
            10 | 11 => {
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
            _ => {
                let hue = ((self.time * 120.0 + idx as f64 * 12.0) % 360.0) as f32;
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

    fn compute_breathing(&self) -> (u8, u8, u8) {
        let phase = (self.time * std::f64::consts::TAU / 4.0).sin();
        let factor = (phase + 1.0) / 2.0;
        (
            (self.params.base_r as f64 * factor) as u8,
            (self.params.base_g as f64 * factor) as u8,
            (self.params.base_b as f64 * factor) as u8,
        )
    }

    fn compute_strobe(&self) -> (u8, u8, u8) {
        let on = ((self.time * 4.0) % 1.0) < 0.5;
        if on {
            (self.params.base_r, self.params.base_g, self.params.base_b)
        } else {
            (0, 0, 0)
        }
    }

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
        let mut out = (0, 0, 0);
        for _ in 0..60 {
            out = engine.tick(0.016);
        }
        let sum = out.0 as u32 + out.1 as u32 + out.2 as u32;
        assert!(sum <= 640);
    }

    #[test]
    fn test_effect_capabilities() {
        assert_eq!(
            Effect::Static.capability(),
            EffectCapability::WholeStripColor
        );
        assert_eq!(
            Effect::HueCycle.capability(),
            EffectCapability::WholeStripColor
        );
        assert_eq!(
            Effect::HardwarePattern(5).capability(),
            EffectCapability::HardwareControllerPattern
        );
    }
}
