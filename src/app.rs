use crate::audio::decoder::AudioData;
use crate::audio::playback::PlaybackState;
use std::sync::Arc;

/// Which mode the UI is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    FilePicker,
}

/// Which panel has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    WorldSliders,
    EffectsSliders,
    Transport,
}

impl PanelFocus {
    /// Cycle to the next panel.
    pub fn next(self) -> Self {
        match self {
            Self::WorldSliders => Self::EffectsSliders,
            Self::EffectsSliders => Self::Transport,
            Self::Transport => Self::WorldSliders,
        }
    }
}

/// Side-effect actions returned by the input handler.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    LoadFile(String),
}

/// Info about the currently loaded file.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub total_samples: usize,
}

/// A single slider with name, range, value, and step.
#[derive(Debug, Clone)]
pub struct SliderDef {
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub default: f64,
    pub step: f64,
    pub unit: &'static str,
}

impl SliderDef {
    /// Adjust the slider value by `delta` steps, clamping to [min, max].
    pub fn adjust(&mut self, steps: f64) {
        self.value = (self.value + steps * self.step).clamp(self.min, self.max);
        // Round to avoid floating-point drift.
        let precision = (1.0 / self.step).round();
        self.value = (self.value * precision).round() / precision;
    }

    /// Fraction [0.0, 1.0] representing where the value sits in the range.
    pub fn fraction(&self) -> f64 {
        if (self.max - self.min).abs() < f64::EPSILON {
            return 0.0;
        }
        (self.value - self.min) / (self.max - self.min)
    }
}

/// All application state for the TUI.
pub struct AppState {
    pub mode: AppMode,
    pub focus: PanelFocus,
    pub selected_slider: usize,
    pub world_sliders: Vec<SliderDef>,
    pub effects_sliders: Vec<SliderDef>,
    pub file_info: Option<FileInfo>,
    pub playback: PlaybackState,
    pub audio_data: Option<Arc<AudioData>>,
    pub loop_enabled: bool,
    pub ab_original: bool,
    pub should_quit: bool,
    pub file_picker_input: String,
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Normal,
            focus: PanelFocus::WorldSliders,
            selected_slider: 0,
            world_sliders: Self::default_world_sliders(),
            effects_sliders: Self::default_effects_sliders(),
            file_info: None,
            playback: PlaybackState::new(),
            audio_data: None,
            loop_enabled: false,
            ab_original: false,
            should_quit: false,
            file_picker_input: String::new(),
            status_message: None,
        }
    }

    fn default_world_sliders() -> Vec<SliderDef> {
        vec![
            SliderDef {
                label: "Pitch Shift",
                min: -12.0,
                max: 12.0,
                value: 0.0,
                default: 0.0,
                step: 0.5,
                unit: "st",
            },
            SliderDef {
                label: "Pitch Range",
                min: 0.2,
                max: 3.0,
                value: 1.0,
                default: 1.0,
                step: 0.1,
                unit: "×",
            },
            SliderDef {
                label: "Speed",
                min: 0.5,
                max: 2.0,
                value: 1.0,
                default: 1.0,
                step: 0.05,
                unit: "×",
            },
            SliderDef {
                label: "Breathiness",
                min: 0.0,
                max: 3.0,
                value: 0.0,
                default: 0.0,
                step: 0.1,
                unit: "×",
            },
            SliderDef {
                label: "Formant Shift",
                min: -5.0,
                max: 5.0,
                value: 0.0,
                default: 0.0,
                step: 0.5,
                unit: "st",
            },
            SliderDef {
                label: "Spectral Tilt",
                min: -6.0,
                max: 6.0,
                value: 0.0,
                default: 0.0,
                step: 0.5,
                unit: "dB/oct",
            },
        ]
    }

    fn default_effects_sliders() -> Vec<SliderDef> {
        vec![
            SliderDef {
                label: "Gain",
                min: -12.0,
                max: 12.0,
                value: 0.0,
                default: 0.0,
                step: 0.5,
                unit: "dB",
            },
            SliderDef {
                label: "Low Cut",
                min: 20.0,
                max: 500.0,
                value: 20.0,
                default: 20.0,
                step: 10.0,
                unit: "Hz",
            },
            SliderDef {
                label: "High Cut",
                min: 2000.0,
                max: 20000.0,
                value: 20000.0,
                default: 20000.0,
                step: 500.0,
                unit: "Hz",
            },
            SliderDef {
                label: "Compressor",
                min: -40.0,
                max: 0.0,
                value: 0.0,
                default: 0.0,
                step: 1.0,
                unit: "dB",
            },
            SliderDef {
                label: "Reverb Mix",
                min: 0.0,
                max: 1.0,
                value: 0.0,
                default: 0.0,
                step: 0.05,
                unit: "",
            },
            SliderDef {
                label: "Pitch Shift FX",
                min: -12.0,
                max: 12.0,
                value: 0.0,
                default: 0.0,
                step: 0.5,
                unit: "st",
            },
        ]
    }

    /// Get the sliders for the currently focused panel.
    pub fn focused_sliders(&self) -> &[SliderDef] {
        match self.focus {
            PanelFocus::WorldSliders => &self.world_sliders,
            PanelFocus::EffectsSliders => &self.effects_sliders,
            PanelFocus::Transport => &[],
        }
    }

    /// Get the mutable sliders for the currently focused panel.
    /// Returns `None` when Transport is focused (no sliders).
    pub fn focused_sliders_mut(&mut self) -> Option<&mut Vec<SliderDef>> {
        match self.focus {
            PanelFocus::WorldSliders => Some(&mut self.world_sliders),
            PanelFocus::EffectsSliders => Some(&mut self.effects_sliders),
            PanelFocus::Transport => None,
        }
    }

    /// Number of sliders in the currently focused panel.
    pub fn focused_slider_count(&self) -> usize {
        self.focused_sliders().len()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
