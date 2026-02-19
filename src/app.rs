use crate::audio::decoder::AudioData;
use crate::audio::playback::PlaybackState;
use crate::dsp::effects::EffectsParams;
use crate::dsp::modifier::WorldSliderValues;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use std::sync::Arc;

/// Which mode the UI is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    FilePicker,
    Saving,
    Help,
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
    Resynthesize,
    ReapplyEffects,
    /// Live gain update — carries pre-computed linear multiplier.
    LiveGain(f32),
    ToggleAB,
    ExportWav(String),
}

/// Info about the currently loaded file.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
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
    ///
    /// Step must be positive (enforced at construction via `new()`).
    pub fn adjust(&mut self, steps: f64) {
        if self.step <= 0.0 || !self.step.is_finite() {
            return; // #5: guard against division by zero / NaN
        }
        self.value = (self.value + steps * self.step).clamp(self.min, self.max);
        // #9: Round to step grid to prevent floating-point drift accumulation.
        let precision = (1.0 / self.step).round();
        if precision > 0.0 && precision.is_finite() {
            self.value = (self.value * precision).round() / precision;
        }
    }

    /// Reset the slider to its default value. Returns true if the value changed.
    pub fn reset(&mut self) -> bool {
        if self.value != self.default {
            self.value = self.default;
            true
        } else {
            false
        }
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
    pub original_audio: Option<Arc<AudioData>>,
    pub processing_status: Option<String>,
    pub loop_enabled: bool,
    pub ab_original: bool,
    pub should_quit: bool,
    pub file_picker_input: String,
    pub status_message: Option<String>,
    pub spectrum_bins: Vec<f32>,
    pub spectrum_picker: Option<Picker>,
    pub spectrum_state: Option<Box<dyn StatefulProtocol>>,
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
            original_audio: None,
            processing_status: None,
            loop_enabled: false,
            ab_original: false,
            should_quit: false,
            file_picker_input: String::new(),
            status_message: None,
            spectrum_bins: Vec::new(),
            spectrum_picker: None,
            spectrum_state: None,
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

    /// Extract current effects slider values.
    pub fn effects_params(&self) -> EffectsParams {
        let s = &self.effects_sliders;
        EffectsParams {
            gain_db: s[0].value as f32,
            low_cut_hz: s[1].value as f32,
            high_cut_hz: s[2].value as f32,
            compressor_thresh_db: s[3].value as f32,
            reverb_mix: s[4].value as f32,
            pitch_shift_semitones: s[5].value as f32,
        }
    }

    /// Extract current WORLD slider values for the modifier.
    pub fn world_slider_values(&self) -> WorldSliderValues {
        let s = &self.world_sliders;
        WorldSliderValues {
            pitch_shift: s[0].value,
            pitch_range: s[1].value,
            speed: s[2].value,
            breathiness: s[3].value,
            formant_shift: s[4].value,
            spectral_tilt: s[5].value,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
