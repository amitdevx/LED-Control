// Music sync and audio reactivity engine.
//
// ponytail: Uses native parec stream or built-in BPM rhythm generator.
// Zero extra crates needed, pure standard library audio processing.

use std::io::Read;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Audio capture status for PulseAudio/PipeWire recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureStatus {
    Inactive,
    Active,
    Failed(String),
}

/// Music sync modes matching the LED LAMP app features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicMode {
    /// Pulses brightness and shifts hue on beat peaks.
    BeatPulse,
    /// Rapid color jumps on every rhythmic hit.
    BeatJump,
    /// Smooth dynamic wave that accelerates with audio energy.
    EnergyWave,
    /// Fast energetic strobe reacting to high peaks.
    RhythmFlash,
}

impl MusicMode {
    pub fn label(&self) -> &'static str {
        match self {
            MusicMode::BeatPulse => "Beat Pulse",
            MusicMode::BeatJump => "Beat Jump",
            MusicMode::EnergyWave => "Energy Wave",
            MusicMode::RhythmFlash => "Rhythm Flash",
        }
    }

    pub fn all() -> &'static [MusicMode] {
        &[
            MusicMode::BeatPulse,
            MusicMode::BeatJump,
            MusicMode::EnergyWave,
            MusicMode::RhythmFlash,
        ]
    }
}

/// Music sync configuration and live state.
pub struct MusicEngine {
    mode: MusicMode,
    sensitivity: f64, // 0.1 to 3.0
    bpm: f64,         // 40.0 to 240.0
    use_live_mic: bool,
    capture_status: CaptureStatus,
    capture_error: Arc<Mutex<Option<String>>>,
    time: f64,
    color_index: usize,
    audio_energy: Arc<Mutex<f64>>,
    audio_active: Arc<AtomicBool>,
    child_process: Option<Child>,
}

impl MusicEngine {
    pub fn new() -> Self {
        Self {
            mode: MusicMode::BeatPulse,
            sensitivity: 1.0,
            bpm: 128.0,
            use_live_mic: false,
            capture_status: CaptureStatus::Inactive,
            capture_error: Arc::new(Mutex::new(None)),
            time: 0.0,
            color_index: 0,
            audio_energy: Arc::new(Mutex::new(0.0)),
            audio_active: Arc::new(AtomicBool::new(false)),
            child_process: None,
        }
    }

    pub fn set_mode(&mut self, mode: MusicMode) {
        self.mode = mode;
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> MusicMode {
        self.mode
    }

    pub fn set_sensitivity(&mut self, s: f64) {
        self.sensitivity = s.clamp(0.1, 3.0);
    }

    #[allow(dead_code)]
    pub fn sensitivity(&self) -> f64 {
        self.sensitivity
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(40.0, 240.0);
    }

    #[allow(dead_code)]
    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    pub fn set_use_live_mic(&mut self, live: bool) {
        if self.use_live_mic == live && !matches!(self.capture_status, CaptureStatus::Failed(_)) {
            return;
        }
        self.use_live_mic = live;
        if live {
            self.start_audio_capture();
        } else {
            self.stop_audio_capture();
        }
    }

    #[allow(dead_code)]
    pub fn is_live_mic(&self) -> bool {
        self.use_live_mic
    }

    pub fn capture_status(&self) -> CaptureStatus {
        if let Ok(slot) = self.capture_error.lock() {
            if let Some(ref err) = *slot {
                return CaptureStatus::Failed(err.clone());
            }
        }
        self.capture_status.clone()
    }

    /// Read current instantaneous energy level (0.0 to 1.0) without advancing time or state.
    pub fn current_energy(&self) -> f64 {
        if self.use_live_mic {
            let val = self.audio_energy.lock().map(|g| *g).unwrap_or(0.0);
            (val * self.sensitivity).clamp(0.0, 1.0)
        } else {
            // Simulated rhythmic beat based on BPM and current phase
            let beat_freq = self.bpm / 60.0;
            let phase = (self.time * beat_freq * std::f64::consts::TAU).sin();
            let beat = if phase > 0.6 {
                ((phase - 0.6) / 0.4).powi(2)
            } else {
                0.0
            };
            (beat * self.sensitivity).clamp(0.0, 1.0)
        }
    }

    /// Advance time and compute the reactive RGB values.
    pub fn tick(&mut self, dt: f64) -> (u8, u8, u8) {
        self.time += dt;
        let energy = self.current_energy();

        const PALETTE: [(u8, u8, u8); 6] = [
            (255, 0, 80),  // Neon Pink
            (0, 240, 255), // Cyan
            (160, 0, 255), // Purple
            (255, 200, 0), // Gold
            (0, 255, 120), // Lime
            (255, 40, 0),  // Electric Red
        ];

        match self.mode {
            MusicMode::BeatPulse => {
                let base = PALETTE[self.color_index % PALETTE.len()];
                if energy > 0.8 && ((self.time * 2.0) as usize) != self.color_index {
                    self.color_index = (self.color_index + 1) % PALETTE.len();
                }
                let factor = (0.2 + energy * 0.8).clamp(0.0, 1.0);
                (
                    (base.0 as f64 * factor) as u8,
                    (base.1 as f64 * factor) as u8,
                    (base.2 as f64 * factor) as u8,
                )
            }
            MusicMode::BeatJump => {
                let step = ((self.time * (self.bpm / 60.0)) as usize) % PALETTE.len();
                let col = PALETTE[step];
                let factor = if energy > 0.4 { 1.0 } else { 0.15 };
                (
                    (col.0 as f64 * factor) as u8,
                    (col.1 as f64 * factor) as u8,
                    (col.2 as f64 * factor) as u8,
                )
            }
            MusicMode::EnergyWave => {
                let hue = (self.time * 60.0 + energy * 180.0) % 360.0;
                let (r, g, b) = hue_to_rgb_simple(hue);
                let factor = (0.4 + energy * 0.6).clamp(0.0, 1.0);
                (
                    (r as f64 * factor) as u8,
                    (g as f64 * factor) as u8,
                    (b as f64 * factor) as u8,
                )
            }
            MusicMode::RhythmFlash => {
                if energy > 0.5 {
                    (255, 255, 255)
                } else if energy > 0.25 {
                    let col = PALETTE[self.color_index % PALETTE.len()];
                    (col.0, col.1, col.2)
                } else {
                    (0, 0, 0)
                }
            }
        }
    }

    fn start_audio_capture(&mut self) {
        self.stop_audio_capture();
        self.audio_active.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.capture_error.lock() {
            *slot = None;
        }

        let energy_clone = self.audio_energy.clone();
        let active_clone = self.audio_active.clone();
        let error_clone = self.capture_error.clone();

        // ponytail: Spawn parec in a background thread to read raw 16-bit PCM samples
        let child = ProcessCommand::new("parec")
            .args(["--format=s16le", "--rate=8000", "--channels=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        match child {
            Ok(mut proc) => {
                if let Some(mut stdout) = proc.stdout.take() {
                    thread::spawn(move || {
                        let mut buf = [0u8; 512];
                        while active_clone.load(Ordering::SeqCst) {
                            match stdout.read(&mut buf) {
                                Ok(n) if n > 0 => {
                                    // Calculate root-mean-square amplitude
                                    let mut sum_sq: f64 = 0.0;
                                    let sample_count = n / 2;
                                    for i in 0..sample_count {
                                        let sample =
                                            i16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]);
                                        let norm = sample as f64 / 32768.0;
                                        sum_sq += norm * norm;
                                    }
                                    if sample_count > 0 {
                                        let rms = (sum_sq / sample_count as f64).sqrt();
                                        let normalized = (rms * 4.0).clamp(0.0, 1.0);
                                        if let Ok(mut energy) = energy_clone.lock() {
                                            // Smooth decay filter
                                            *energy = (*energy * 0.7).max(normalized);
                                        }
                                    }
                                }
                                _ => {
                                    if active_clone.load(Ordering::SeqCst) {
                                        if let Ok(mut slot) = error_clone.lock() {
                                            *slot = Some(
                                                "Audio stream ended unexpectedly (parec exited)"
                                                    .to_string(),
                                            );
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    });
                    self.child_process = Some(proc);
                    self.capture_status = CaptureStatus::Active;
                } else {
                    self.capture_status =
                        CaptureStatus::Failed("Failed to capture parec stdout".to_string());
                    self.use_live_mic = false;
                }
            }
            Err(e) => {
                log::warn!("Audio capture process failed to spawn: {e}");
                self.capture_status = CaptureStatus::Failed(format!("parec unavailable: {e}"));
                self.use_live_mic = false;
            }
        }
    }

    fn stop_audio_capture(&mut self) {
        self.audio_active.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.capture_error.lock() {
            *slot = None;
        }
        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.capture_status = CaptureStatus::Inactive;
    }
}

impl Drop for MusicEngine {
    fn drop(&mut self) {
        self.stop_audio_capture();
    }
}

/// Convert hue (0-360) to RGB (0-255).
fn hue_to_rgb_simple(hue: f64) -> (u8, u8, u8) {
    let h = (hue % 360.0) / 60.0;
    let x = (1.0 - (h % 2.0 - 1.0).abs()).clamp(0.0, 1.0);
    let (r, g, b) = match h as usize {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_music_engine_modes_and_tick() {
        let mut engine = MusicEngine::new();
        engine.set_bpm(120.0);
        assert_eq!(engine.bpm(), 120.0);

        for mode in MusicMode::all() {
            engine.set_mode(*mode);
            let (r, g, b) = engine.tick(0.05);
            let _ = (r, g, b);
        }
    }

    #[test]
    fn test_current_energy_does_not_advance_time() {
        let mut engine = MusicEngine::new();
        engine.set_bpm(120.0);
        let e1 = engine.current_energy();
        let e2 = engine.current_energy();
        assert_eq!(e1, e2);
        assert_eq!(engine.time, 0.0);

        let _ = engine.tick(0.1);
        assert!((engine.time - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_sensitivity_clamping() {
        let mut engine = MusicEngine::new();
        engine.set_sensitivity(5.0);
        assert_eq!(engine.sensitivity(), 3.0);
        engine.set_sensitivity(0.01);
        assert_eq!(engine.sensitivity(), 0.1);
    }

    #[test]
    fn test_capture_status_default() {
        let engine = MusicEngine::new();
        assert_eq!(engine.capture_status(), CaptureStatus::Inactive);
    }
}
