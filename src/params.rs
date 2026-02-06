//! Parameter definitions and atomic storage for the Tension Field plugin.

use std::ffi::CStr;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};

use toybox::clack_extensions::params::{ParamDisplayWriter, ParamInfoFlags, ParamInfoWriter};
use toybox::clack_plugin::prelude::ClapId;
use toybox::clap::params::ParamBuilder;

const ROUTE_DEST_COUNT: usize = 6;

/// Pull gesture shape choices.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PullShape {
    /// Constant-rate drag.
    Linear,
    /// Smooth ease-in/out spring profile.
    Rubber,
    /// Quantized steps with soft transitions.
    Ratchet,
    /// Slow sinusoidal pull.
    Wave,
    /// Pulsed staccato shape with short high-tension windows.
    Pulse,
}

impl PullShape {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Rubber,
            2 => Self::Ratchet,
            3 => Self::Wave,
            4 => Self::Pulse,
            _ => Self::Linear,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Linear => 0.0,
            Self::Rubber => 1.0,
            Self::Ratchet => 2.0,
            Self::Wave => 3.0,
            Self::Pulse => 4.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Rubber => "Rubber",
            Self::Ratchet => "Ratchet",
            Self::Wave => "Wave",
            Self::Pulse => "Pulse",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "linear" => Some(Self::Linear),
            "1" | "rubber" => Some(Self::Rubber),
            "2" | "ratchet" => Some(Self::Ratchet),
            "3" | "wave" => Some(Self::Wave),
            "4" | "pulse" => Some(Self::Pulse),
            _ => None,
        }
    }
}

/// Gesture timing source.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum TimeMode {
    /// Pull rate follows free-running Hertz values.
    FreeHz,
    /// Pull rate follows host-synced musical divisions.
    SyncDivision,
}

impl TimeMode {
    fn from_value(value: f32) -> Self {
        if value >= 0.5 {
            Self::SyncDivision
        } else {
            Self::FreeHz
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::FreeHz => 0.0,
            Self::SyncDivision => 1.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FreeHz => "Free Hz",
            Self::SyncDivision => "Sync Div",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "free" | "free hz" | "hz" => Some(Self::FreeHz),
            "1" | "sync" | "division" | "sync div" => Some(Self::SyncDivision),
            _ => None,
        }
    }
}

/// Musical pull-rate divisions used in sync mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PullDivision {
    /// 1/16 note.
    Div1_16,
    /// 1/8 triplet.
    Div1_8T,
    /// 1/8 note.
    Div1_8,
    /// 1/4 triplet.
    Div1_4T,
    /// 1/4 note.
    Div1_4,
    /// 1/2 note.
    Div1_2,
    /// One full bar in 4/4.
    Div1Bar,
    /// Two bars in 4/4.
    Div2Bar,
}

impl PullDivision {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Div1_8T,
            2 => Self::Div1_8,
            3 => Self::Div1_4T,
            4 => Self::Div1_4,
            5 => Self::Div1_2,
            6 => Self::Div1Bar,
            7 => Self::Div2Bar,
            _ => Self::Div1_16,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Div1_16 => 0.0,
            Self::Div1_8T => 1.0,
            Self::Div1_8 => 2.0,
            Self::Div1_4T => 3.0,
            Self::Div1_4 => 4.0,
            Self::Div1_2 => 5.0,
            Self::Div1Bar => 6.0,
            Self::Div2Bar => 7.0,
        }
    }

    /// Return cycle length in quarter-note beats.
    pub(crate) fn beats_per_cycle(self) -> f32 {
        match self {
            Self::Div1_16 => 0.25,
            Self::Div1_8T => 1.0 / 3.0,
            Self::Div1_8 => 0.5,
            Self::Div1_4T => 2.0 / 3.0,
            Self::Div1_4 => 1.0,
            Self::Div1_2 => 2.0,
            Self::Div1Bar => 4.0,
            Self::Div2Bar => 8.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Div1_16 => "1/16",
            Self::Div1_8T => "1/8T",
            Self::Div1_8 => "1/8",
            Self::Div1_4T => "1/4T",
            Self::Div1_4 => "1/4",
            Self::Div1_2 => "1/2",
            Self::Div1Bar => "1 Bar",
            Self::Div2Bar => "2 Bar",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "1/16" => Some(Self::Div1_16),
            "1" | "1/8t" => Some(Self::Div1_8T),
            "2" | "1/8" => Some(Self::Div1_8),
            "3" | "1/4t" => Some(Self::Div1_4T),
            "4" | "1/4" => Some(Self::Div1_4),
            "5" | "1/2" => Some(Self::Div1_2),
            "6" | "1 bar" | "1bar" => Some(Self::Div1Bar),
            "7" | "2 bar" | "2bar" => Some(Self::Div2Bar),
            _ => None,
        }
    }
}

/// Quantization amount for pull trigger launches.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PullQuantize {
    /// Trigger immediately.
    None,
    /// Quantize to sixteenth notes.
    Div1_16,
    /// Quantize to eighth notes.
    Div1_8,
    /// Quantize to quarter notes.
    Div1_4,
}

impl PullQuantize {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Div1_16,
            2 => Self::Div1_8,
            3 => Self::Div1_4,
            _ => Self::None,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Div1_16 => 1.0,
            Self::Div1_8 => 2.0,
            Self::Div1_4 => 3.0,
        }
    }

    /// Return quantization grid spacing in quarter-note beats.
    pub(crate) fn beats(self) -> Option<f32> {
        match self {
            Self::None => None,
            Self::Div1_16 => Some(0.25),
            Self::Div1_8 => Some(0.5),
            Self::Div1_4 => Some(1.0),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Div1_16 => "1/16",
            Self::Div1_8 => "1/8",
            Self::Div1_4 => "1/4",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "none" => Some(Self::None),
            "1" | "1/16" => Some(Self::Div1_16),
            "2" | "1/8" => Some(Self::Div1_8),
            "3" | "1/4" => Some(Self::Div1_4),
            _ => None,
        }
    }
}

/// Spectral color families for the warp stage.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum WarpColor {
    /// Balanced damping and compensation.
    Neutral,
    /// Darker, heavier drag.
    DarkDrag,
    /// Brighter, shearing motion.
    BrightShear,
}

impl WarpColor {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::DarkDrag,
            2 => Self::BrightShear,
            _ => Self::Neutral,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Neutral => 0.0,
            Self::DarkDrag => 1.0,
            Self::BrightShear => 2.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::DarkDrag => "Dark Drag",
            Self::BrightShear => "Bright Shear",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "neutral" => Some(Self::Neutral),
            "1" | "dark" | "dark drag" => Some(Self::DarkDrag),
            "2" | "bright" | "bright shear" => Some(Self::BrightShear),
            _ => None,
        }
    }
}

/// Character modes for the elastic and warp processing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum CharacterMode {
    /// Cleanest processing path.
    Clean,
    /// Adds subtle noise and stronger drift movement.
    Dirty,
    /// Adds dirty behavior plus lightweight sample quantization.
    Crush,
}

impl CharacterMode {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Dirty,
            2 => Self::Crush,
            _ => Self::Clean,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Clean => 0.0,
            Self::Dirty => 1.0,
            Self::Crush => 2.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Clean => "Clean",
            Self::Dirty => "Dirty",
            Self::Crush => "Crush",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "clean" => Some(Self::Clean),
            "1" | "dirty" => Some(Self::Dirty),
            "2" | "crush" => Some(Self::Crush),
            _ => None,
        }
    }
}

/// Shape options for modulation sources.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ModSourceShape {
    /// Sine LFO.
    Sine,
    /// Triangle LFO.
    Triangle,
    /// Smoothed random walk.
    RandomWalk,
    /// Audio input envelope follower.
    Envelope,
}

impl ModSourceShape {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Triangle,
            2 => Self::RandomWalk,
            3 => Self::Envelope,
            _ => Self::Sine,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Sine => 0.0,
            Self::Triangle => 1.0,
            Self::RandomWalk => 2.0,
            Self::Envelope => 3.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::RandomWalk => "Random Walk",
            Self::Envelope => "Envelope",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "sine" => Some(Self::Sine),
            "1" | "triangle" => Some(Self::Triangle),
            "2" | "random" | "random walk" | "walk" => Some(Self::RandomWalk),
            "3" | "env" | "envelope" => Some(Self::Envelope),
            _ => None,
        }
    }
}

/// Timing mode for modulation source rates.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ModRateMode {
    /// Free-running Hertz rate.
    FreeHz,
    /// Host-synced musical division.
    SyncDivision,
}

impl ModRateMode {
    fn from_value(value: f32) -> Self {
        if value >= 0.5 {
            Self::SyncDivision
        } else {
            Self::FreeHz
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::FreeHz => 0.0,
            Self::SyncDivision => 1.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FreeHz => "Free Hz",
            Self::SyncDivision => "Sync Div",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "free" | "hz" => Some(Self::FreeHz),
            "1" | "sync" | "division" => Some(Self::SyncDivision),
            _ => None,
        }
    }
}

/// One modulation source configuration.
#[derive(Debug, Copy, Clone)]
pub(crate) struct ModSourceSettings {
    /// Selected waveform/source type.
    pub shape: ModSourceShape,
    /// Free or synced rate mode.
    pub rate_mode: ModRateMode,
    /// Free-running rate in Hertz.
    pub rate_hz: f32,
    /// Synced rate in beat divisions.
    pub rate_division: PullDivision,
    /// Output depth applied before route depths.
    pub depth: f32,
}

/// Modulation matrix settings used by the DSP engine.
#[derive(Debug, Copy, Clone)]
pub(crate) struct ModSettings {
    /// Whether modulation processing is active.
    pub run: bool,
    /// Source A configuration.
    pub source_a: ModSourceSettings,
    /// Source B configuration.
    pub source_b: ModSourceSettings,
    /// Route depths for sources x destinations.
    pub route_depths: [[f32; ROUTE_DEST_COUNT]; 2],
}

/// Snapshot of all parameters used by the DSP engine.
#[derive(Debug, Copy, Clone)]
pub(crate) struct TensionFieldSettings {
    /// Overall stretching force.
    pub tension: f32,
    /// Biases where cycle tension energy concentrates (early vs late in phase).
    pub tension_bias: f32,
    /// Free-running or host-synced pull timing.
    pub time_mode: TimeMode,
    /// Gesture rate in Hertz for free mode.
    pub pull_rate_hz: f32,
    /// Gesture rate division for sync mode.
    pub pull_division: PullDivision,
    /// Timing swing amount for sync mode.
    pub swing: f32,
    /// Pull profile mode.
    pub pull_shape: PullShape,
    /// Momentary pull trigger.
    pub pull_trigger: bool,
    /// Latching pull mode.
    pub pull_latch: bool,
    /// Quantization amount for pull launches.
    pub pull_quantize: PullQuantize,
    /// Release rebound amount.
    pub rebound: f32,
    /// Shapes how sharply pull energy drops after release.
    pub release_snap: f32,
    /// Pull direction from backward to forward.
    pub pull_direction: f32,
    /// Viscous-to-spring behavior amount.
    pub elasticity: f32,
    /// Continuity-to-grain texture macro.
    pub grain_continuity: f32,
    /// Amount of pitch-following behavior.
    pub pitch_coupling: f32,
    /// Warp spectral color profile.
    pub warp_color: WarpColor,
    /// Warp movement amount.
    pub warp_motion: f32,
    /// Stereo decorrelation amount.
    pub width: f32,
    /// Diffusion density amount.
    pub diffusion: f32,
    /// High-frequency damping amount.
    pub air_damping: f32,
    /// High-frequency compensation toggle.
    pub air_compensation: bool,
    /// Character mode.
    pub character: CharacterMode,
    /// Controlled feedback amount.
    pub feedback: f32,
    /// Input-reactive feedback ducking.
    pub ducking: f32,
    /// Output trim in decibels.
    pub output_trim_db: f32,
    /// Soft safety amount that attenuates excessive energy build-up.
    pub energy_ceiling: f32,
    /// Modulation matrix runtime configuration.
    pub modulation: ModSettings,
}

/// Thread-safe parameter storage.
pub(crate) struct TensionFieldParams {
    tension: AtomicF32,
    tension_bias: AtomicF32,
    pull_rate_hz: AtomicF32,
    pull_shape: AtomicF32,
    hold: AtomicU32,
    grain_continuity: AtomicF32,
    pitch_coupling: AtomicF32,
    width: AtomicF32,
    diffusion: AtomicF32,
    air_damping: AtomicF32,
    air_compensation: AtomicU32,
    pull_direction: AtomicF32,
    elasticity: AtomicF32,
    pull_trigger: AtomicU32,
    rebound: AtomicF32,
    release_snap: AtomicF32,
    clean_dirty: AtomicF32,
    feedback: AtomicF32,
    time_mode: AtomicF32,
    pull_division: AtomicF32,
    swing: AtomicF32,
    pull_latch: AtomicU32,
    pull_quantize: AtomicF32,
    warp_color: AtomicF32,
    warp_motion: AtomicF32,
    ducking: AtomicF32,
    output_trim_db: AtomicF32,
    energy_ceiling: AtomicF32,
    mod_run: AtomicU32,
    mod_a_shape: AtomicF32,
    mod_a_rate_mode: AtomicF32,
    mod_a_rate_hz: AtomicF32,
    mod_a_division: AtomicF32,
    mod_a_depth: AtomicF32,
    mod_b_shape: AtomicF32,
    mod_b_rate_mode: AtomicF32,
    mod_b_rate_hz: AtomicF32,
    mod_b_division: AtomicF32,
    mod_b_depth: AtomicF32,
    mod_route_a: [AtomicF32; ROUTE_DEST_COUNT],
    mod_route_b: [AtomicF32; ROUTE_DEST_COUNT],
}

impl TensionFieldParams {
    /// Create a new parameter store with default values.
    pub(crate) fn new() -> Self {
        Self {
            tension: AtomicF32::new(0.5),
            tension_bias: AtomicF32::new(0.5),
            pull_rate_hz: AtomicF32::new(0.35),
            pull_shape: AtomicF32::new(PullShape::Rubber.as_value()),
            hold: AtomicU32::new(0),
            grain_continuity: AtomicF32::new(0.28),
            pitch_coupling: AtomicF32::new(0.2),
            width: AtomicF32::new(0.6),
            diffusion: AtomicF32::new(0.55),
            air_damping: AtomicF32::new(0.35),
            air_compensation: AtomicU32::new(1),
            pull_direction: AtomicF32::new(0.5),
            elasticity: AtomicF32::new(0.65),
            pull_trigger: AtomicU32::new(0),
            rebound: AtomicF32::new(0.55),
            release_snap: AtomicF32::new(0.35),
            clean_dirty: AtomicF32::new(CharacterMode::Clean.as_value()),
            feedback: AtomicF32::new(0.12),
            time_mode: AtomicF32::new(TimeMode::SyncDivision.as_value()),
            pull_division: AtomicF32::new(PullDivision::Div1_4.as_value()),
            swing: AtomicF32::new(0.0),
            pull_latch: AtomicU32::new(0),
            pull_quantize: AtomicF32::new(PullQuantize::Div1_16.as_value()),
            warp_color: AtomicF32::new(WarpColor::Neutral.as_value()),
            warp_motion: AtomicF32::new(0.35),
            ducking: AtomicF32::new(0.0),
            output_trim_db: AtomicF32::new(0.0),
            energy_ceiling: AtomicF32::new(0.7),
            mod_run: AtomicU32::new(1),
            mod_a_shape: AtomicF32::new(ModSourceShape::Sine.as_value()),
            mod_a_rate_mode: AtomicF32::new(ModRateMode::SyncDivision.as_value()),
            mod_a_rate_hz: AtomicF32::new(0.18),
            mod_a_division: AtomicF32::new(PullDivision::Div1_2.as_value()),
            mod_a_depth: AtomicF32::new(0.22),
            mod_b_shape: AtomicF32::new(ModSourceShape::RandomWalk.as_value()),
            mod_b_rate_mode: AtomicF32::new(ModRateMode::SyncDivision.as_value()),
            mod_b_rate_hz: AtomicF32::new(0.09),
            mod_b_division: AtomicF32::new(PullDivision::Div1Bar.as_value()),
            mod_b_depth: AtomicF32::new(0.2),
            mod_route_a: [
                AtomicF32::new(0.35),
                AtomicF32::new(0.25),
                AtomicF32::new(0.0),
                AtomicF32::new(0.0),
                AtomicF32::new(0.0),
                AtomicF32::new(0.0),
            ],
            mod_route_b: [
                AtomicF32::new(0.0),
                AtomicF32::new(0.0),
                AtomicF32::new(0.25),
                AtomicF32::new(0.18),
                AtomicF32::new(0.2),
                AtomicF32::new(0.0),
            ],
        }
    }

    /// Apply a single parameter update from CLAP automation.
    pub(crate) fn set_param(&self, param_id: ClapId, value: f32) {
        match param_id {
            PARAM_TENSION_ID => self.tension.store(clamp(value, 0.0, 1.0)),
            PARAM_TENSION_BIAS_ID => self.tension_bias.store(clamp(value, 0.0, 1.0)),
            PARAM_PULL_RATE_ID => self.pull_rate_hz.store(clamp(value, 0.02, 4.0)),
            PARAM_PULL_SHAPE_ID => self.pull_shape.store(clamp(value, 0.0, 4.0).round()),
            PARAM_HOLD_ID => self
                .hold
                .store(bool_to_u32(value >= 0.5), Ordering::Relaxed),
            PARAM_GRAIN_CONTINUITY_ID => self.grain_continuity.store(clamp(value, 0.0, 1.0)),
            PARAM_PITCH_COUPLING_ID => self.pitch_coupling.store(clamp(value, 0.0, 1.0)),
            PARAM_WIDTH_ID => self.width.store(clamp(value, 0.0, 1.0)),
            PARAM_DIFFUSION_ID => self.diffusion.store(clamp(value, 0.0, 1.0)),
            PARAM_AIR_DAMPING_ID => self.air_damping.store(clamp(value, 0.0, 1.0)),
            PARAM_AIR_COMP_ID => self
                .air_compensation
                .store(bool_to_u32(value >= 0.5), Ordering::Relaxed),
            PARAM_PULL_DIRECTION_ID => self.pull_direction.store(clamp(value, 0.0, 1.0)),
            PARAM_ELASTICITY_ID => self.elasticity.store(clamp(value, 0.0, 1.0)),
            PARAM_PULL_TRIGGER_ID => self
                .pull_trigger
                .store(bool_to_u32(value >= 0.5), Ordering::Relaxed),
            PARAM_REBOUND_ID => self.rebound.store(clamp(value, 0.0, 1.0)),
            PARAM_RELEASE_SNAP_ID => self.release_snap.store(clamp(value, 0.0, 1.0)),
            PARAM_CLEAN_DIRTY_ID => self.clean_dirty.store(clamp(value, 0.0, 2.0).round()),
            PARAM_FEEDBACK_ID => self.feedback.store(clamp(value, 0.0, 0.7)),
            PARAM_TIME_MODE_ID => self.time_mode.store(clamp(value, 0.0, 1.0).round()),
            PARAM_PULL_DIVISION_ID => self.pull_division.store(clamp(value, 0.0, 7.0).round()),
            PARAM_SWING_ID => self.swing.store(clamp(value, 0.0, 1.0)),
            PARAM_PULL_LATCH_ID => self
                .pull_latch
                .store(bool_to_u32(value >= 0.5), Ordering::Relaxed),
            PARAM_PULL_QUANTIZE_ID => self.pull_quantize.store(clamp(value, 0.0, 3.0).round()),
            PARAM_WARP_COLOR_ID => self.warp_color.store(clamp(value, 0.0, 2.0).round()),
            PARAM_WARP_MOTION_ID => self.warp_motion.store(clamp(value, 0.0, 1.0)),
            PARAM_DUCKING_ID => self.ducking.store(clamp(value, 0.0, 1.0)),
            PARAM_OUTPUT_TRIM_DB_ID => self.output_trim_db.store(clamp(value, -12.0, 6.0)),
            PARAM_ENERGY_CEILING_ID => self.energy_ceiling.store(clamp(value, 0.0, 1.0)),
            PARAM_MOD_RUN_ID => self
                .mod_run
                .store(bool_to_u32(value >= 0.5), Ordering::Relaxed),
            PARAM_MOD_A_SHAPE_ID => self.mod_a_shape.store(clamp(value, 0.0, 3.0).round()),
            PARAM_MOD_A_RATE_MODE_ID => self.mod_a_rate_mode.store(clamp(value, 0.0, 1.0).round()),
            PARAM_MOD_A_RATE_HZ_ID => self.mod_a_rate_hz.store(clamp(value, 0.01, 4.0)),
            PARAM_MOD_A_DIVISION_ID => self.mod_a_division.store(clamp(value, 0.0, 7.0).round()),
            PARAM_MOD_A_DEPTH_ID => self.mod_a_depth.store(clamp(value, 0.0, 1.0)),
            PARAM_MOD_B_SHAPE_ID => self.mod_b_shape.store(clamp(value, 0.0, 3.0).round()),
            PARAM_MOD_B_RATE_MODE_ID => self.mod_b_rate_mode.store(clamp(value, 0.0, 1.0).round()),
            PARAM_MOD_B_RATE_HZ_ID => self.mod_b_rate_hz.store(clamp(value, 0.01, 4.0)),
            PARAM_MOD_B_DIVISION_ID => self.mod_b_division.store(clamp(value, 0.0, 7.0).round()),
            PARAM_MOD_B_DEPTH_ID => self.mod_b_depth.store(clamp(value, 0.0, 1.0)),
            PARAM_MOD_A_TO_TENSION_ID => self.mod_route_a[0].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_A_TO_DIRECTION_ID => self.mod_route_a[1].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_A_TO_GRAIN_ID => self.mod_route_a[2].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_A_TO_WIDTH_ID => self.mod_route_a[3].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_A_TO_WARP_MOTION_ID => self.mod_route_a[4].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_A_TO_FEEDBACK_ID => self.mod_route_a[5].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_TENSION_ID => self.mod_route_b[0].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_DIRECTION_ID => self.mod_route_b[1].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_GRAIN_ID => self.mod_route_b[2].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_WIDTH_ID => self.mod_route_b[3].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_WARP_MOTION_ID => self.mod_route_b[4].store(clamp(value, -1.0, 1.0)),
            PARAM_MOD_B_TO_FEEDBACK_ID => self.mod_route_b[5].store(clamp(value, -1.0, 1.0)),
            _ => {}
        }
    }

    /// Fetch a parameter value for host reads.
    pub(crate) fn get_param(&self, param_id: ClapId) -> Option<f32> {
        match param_id {
            PARAM_TENSION_ID => Some(self.tension.load()),
            PARAM_TENSION_BIAS_ID => Some(self.tension_bias.load()),
            PARAM_PULL_RATE_ID => Some(self.pull_rate_hz.load()),
            PARAM_PULL_SHAPE_ID => Some(self.pull_shape.load()),
            PARAM_HOLD_ID => Some(u32_to_bool(self.hold.load(Ordering::Relaxed)) as u8 as f32),
            PARAM_GRAIN_CONTINUITY_ID => Some(self.grain_continuity.load()),
            PARAM_PITCH_COUPLING_ID => Some(self.pitch_coupling.load()),
            PARAM_WIDTH_ID => Some(self.width.load()),
            PARAM_DIFFUSION_ID => Some(self.diffusion.load()),
            PARAM_AIR_DAMPING_ID => Some(self.air_damping.load()),
            PARAM_AIR_COMP_ID => {
                Some(u32_to_bool(self.air_compensation.load(Ordering::Relaxed)) as u8 as f32)
            }
            PARAM_PULL_DIRECTION_ID => Some(self.pull_direction.load()),
            PARAM_ELASTICITY_ID => Some(self.elasticity.load()),
            PARAM_PULL_TRIGGER_ID => {
                Some(u32_to_bool(self.pull_trigger.load(Ordering::Relaxed)) as u8 as f32)
            }
            PARAM_REBOUND_ID => Some(self.rebound.load()),
            PARAM_RELEASE_SNAP_ID => Some(self.release_snap.load()),
            PARAM_CLEAN_DIRTY_ID => Some(self.clean_dirty.load()),
            PARAM_FEEDBACK_ID => Some(self.feedback.load()),
            PARAM_TIME_MODE_ID => Some(self.time_mode.load()),
            PARAM_PULL_DIVISION_ID => Some(self.pull_division.load()),
            PARAM_SWING_ID => Some(self.swing.load()),
            PARAM_PULL_LATCH_ID => {
                Some(u32_to_bool(self.pull_latch.load(Ordering::Relaxed)) as u8 as f32)
            }
            PARAM_PULL_QUANTIZE_ID => Some(self.pull_quantize.load()),
            PARAM_WARP_COLOR_ID => Some(self.warp_color.load()),
            PARAM_WARP_MOTION_ID => Some(self.warp_motion.load()),
            PARAM_DUCKING_ID => Some(self.ducking.load()),
            PARAM_OUTPUT_TRIM_DB_ID => Some(self.output_trim_db.load()),
            PARAM_ENERGY_CEILING_ID => Some(self.energy_ceiling.load()),
            PARAM_MOD_RUN_ID => {
                Some(u32_to_bool(self.mod_run.load(Ordering::Relaxed)) as u8 as f32)
            }
            PARAM_MOD_A_SHAPE_ID => Some(self.mod_a_shape.load()),
            PARAM_MOD_A_RATE_MODE_ID => Some(self.mod_a_rate_mode.load()),
            PARAM_MOD_A_RATE_HZ_ID => Some(self.mod_a_rate_hz.load()),
            PARAM_MOD_A_DIVISION_ID => Some(self.mod_a_division.load()),
            PARAM_MOD_A_DEPTH_ID => Some(self.mod_a_depth.load()),
            PARAM_MOD_B_SHAPE_ID => Some(self.mod_b_shape.load()),
            PARAM_MOD_B_RATE_MODE_ID => Some(self.mod_b_rate_mode.load()),
            PARAM_MOD_B_RATE_HZ_ID => Some(self.mod_b_rate_hz.load()),
            PARAM_MOD_B_DIVISION_ID => Some(self.mod_b_division.load()),
            PARAM_MOD_B_DEPTH_ID => Some(self.mod_b_depth.load()),
            PARAM_MOD_A_TO_TENSION_ID => Some(self.mod_route_a[0].load()),
            PARAM_MOD_A_TO_DIRECTION_ID => Some(self.mod_route_a[1].load()),
            PARAM_MOD_A_TO_GRAIN_ID => Some(self.mod_route_a[2].load()),
            PARAM_MOD_A_TO_WIDTH_ID => Some(self.mod_route_a[3].load()),
            PARAM_MOD_A_TO_WARP_MOTION_ID => Some(self.mod_route_a[4].load()),
            PARAM_MOD_A_TO_FEEDBACK_ID => Some(self.mod_route_a[5].load()),
            PARAM_MOD_B_TO_TENSION_ID => Some(self.mod_route_b[0].load()),
            PARAM_MOD_B_TO_DIRECTION_ID => Some(self.mod_route_b[1].load()),
            PARAM_MOD_B_TO_GRAIN_ID => Some(self.mod_route_b[2].load()),
            PARAM_MOD_B_TO_WIDTH_ID => Some(self.mod_route_b[3].load()),
            PARAM_MOD_B_TO_WARP_MOTION_ID => Some(self.mod_route_b[4].load()),
            PARAM_MOD_B_TO_FEEDBACK_ID => Some(self.mod_route_b[5].load()),
            _ => None,
        }
    }

    /// Build an immutable settings snapshot for one audio block.
    pub(crate) fn settings(&self) -> TensionFieldSettings {
        let route_a = std::array::from_fn(|index| self.mod_route_a[index].load());
        let route_b = std::array::from_fn(|index| self.mod_route_b[index].load());

        TensionFieldSettings {
            tension: self.tension.load(),
            tension_bias: self.tension_bias.load(),
            time_mode: TimeMode::from_value(self.time_mode.load()),
            pull_rate_hz: self.pull_rate_hz.load(),
            pull_division: PullDivision::from_value(self.pull_division.load()),
            swing: self.swing.load(),
            pull_shape: PullShape::from_value(self.pull_shape.load()),
            pull_trigger: u32_to_bool(self.pull_trigger.load(Ordering::Relaxed)),
            pull_latch: u32_to_bool(self.pull_latch.load(Ordering::Relaxed))
                || u32_to_bool(self.hold.load(Ordering::Relaxed)),
            pull_quantize: PullQuantize::from_value(self.pull_quantize.load()),
            rebound: self.rebound.load(),
            release_snap: self.release_snap.load(),
            pull_direction: self.pull_direction.load() * 2.0 - 1.0,
            elasticity: self.elasticity.load(),
            grain_continuity: self.grain_continuity.load(),
            pitch_coupling: self.pitch_coupling.load(),
            warp_color: WarpColor::from_value(self.warp_color.load()),
            warp_motion: self.warp_motion.load(),
            width: self.width.load(),
            diffusion: self.diffusion.load(),
            air_damping: self.air_damping.load(),
            air_compensation: u32_to_bool(self.air_compensation.load(Ordering::Relaxed)),
            character: CharacterMode::from_value(self.clean_dirty.load()),
            feedback: self.feedback.load(),
            ducking: self.ducking.load(),
            output_trim_db: self.output_trim_db.load(),
            energy_ceiling: self.energy_ceiling.load(),
            modulation: ModSettings {
                run: u32_to_bool(self.mod_run.load(Ordering::Relaxed)),
                source_a: ModSourceSettings {
                    shape: ModSourceShape::from_value(self.mod_a_shape.load()),
                    rate_mode: ModRateMode::from_value(self.mod_a_rate_mode.load()),
                    rate_hz: self.mod_a_rate_hz.load(),
                    rate_division: PullDivision::from_value(self.mod_a_division.load()),
                    depth: self.mod_a_depth.load(),
                },
                source_b: ModSourceSettings {
                    shape: ModSourceShape::from_value(self.mod_b_shape.load()),
                    rate_mode: ModRateMode::from_value(self.mod_b_rate_mode.load()),
                    rate_hz: self.mod_b_rate_hz.load(),
                    rate_division: PullDivision::from_value(self.mod_b_division.load()),
                    depth: self.mod_b_depth.load(),
                },
                route_depths: [route_a, route_b],
            },
        }
    }
}

/// Convert a pull-shape index to an internal shape value.
#[cfg(target_os = "windows")]
pub(crate) fn pull_shape_value_from_index(index: usize) -> f32 {
    match index {
        1 => PullShape::Rubber.as_value(),
        2 => PullShape::Ratchet.as_value(),
        3 => PullShape::Wave.as_value(),
        4 => PullShape::Pulse.as_value(),
        _ => PullShape::Linear.as_value(),
    }
}

/// Convert a pull-division index to an internal division value.
#[cfg(target_os = "windows")]
pub(crate) fn pull_division_value_from_index(index: usize) -> f32 {
    index.min(7) as f32
}

/// Convert a pull-quantize index to an internal quantize value.
#[cfg(target_os = "windows")]
pub(crate) fn pull_quantize_value_from_index(index: usize) -> f32 {
    index.min(3) as f32
}

/// Convert a warp-color index to an internal color value.
#[cfg(target_os = "windows")]
pub(crate) fn warp_color_value_from_index(index: usize) -> f32 {
    index.min(2) as f32
}

/// Convert a character-mode index to an internal mode value.
#[cfg(target_os = "windows")]
pub(crate) fn character_mode_value_from_index(index: usize) -> f32 {
    index.min(2) as f32
}

/// Convert a modulation source shape index to an internal shape value.
#[cfg(target_os = "windows")]
pub(crate) fn mod_source_shape_value_from_index(index: usize) -> f32 {
    index.min(3) as f32
}

/// Convert a modulation rate-mode index to an internal mode value.
#[cfg(target_os = "windows")]
pub(crate) fn mod_rate_mode_value_from_index(index: usize) -> f32 {
    index.min(1) as f32
}

/// Return the number of host-visible parameters.
pub(crate) fn param_count() -> u32 {
    PARAM_DEFS.len() as u32
}

/// Number of serialized parameter values stored in plugin state.
pub(crate) const STATE_VALUE_COUNT: usize = PARAM_DEFS.len();

/// Build a default ordered parameter snapshot in `PARAM_DEFS` order.
pub(crate) fn default_state_values() -> [f32; STATE_VALUE_COUNT] {
    let mut values = [0.0; STATE_VALUE_COUNT];
    for (index, def) in PARAM_DEFS.iter().enumerate() {
        values[index] = def.default_value as f32;
    }
    values
}

/// Build a stable, ordered parameter snapshot for CLAP state serialization.
pub(crate) fn state_values(params: &TensionFieldParams) -> [f32; STATE_VALUE_COUNT] {
    let mut values = default_state_values();
    for (index, def) in PARAM_DEFS.iter().enumerate() {
        values[index] = params.get_param(def.id).unwrap_or(def.default_value as f32);
    }
    values
}

/// Apply a serialized parameter snapshot to the live parameter store.
pub(crate) fn apply_state_values(params: &TensionFieldParams, values: [f32; STATE_VALUE_COUNT]) {
    for (index, def) in PARAM_DEFS.iter().enumerate() {
        params.set_param(def.id, values[index]);
    }
}

/// Write parameter metadata for one parameter index.
pub(crate) fn write_param_info(param_index: u32, writer: &mut ParamInfoWriter) {
    let Some(def) = PARAM_DEFS.get(param_index as usize) else {
        return;
    };
    def.to_spec().write(writer);
}

/// Format a parameter value for host displays.
pub(crate) fn value_to_text(
    param_id: ClapId,
    value: f64,
    writer: &mut ParamDisplayWriter,
) -> std::fmt::Result {
    match param_id {
        PARAM_TENSION_ID
        | PARAM_TENSION_BIAS_ID
        | PARAM_GRAIN_CONTINUITY_ID
        | PARAM_PITCH_COUPLING_ID
        | PARAM_WIDTH_ID
        | PARAM_DIFFUSION_ID
        | PARAM_AIR_DAMPING_ID
        | PARAM_ELASTICITY_ID
        | PARAM_REBOUND_ID
        | PARAM_RELEASE_SNAP_ID
        | PARAM_FEEDBACK_ID
        | PARAM_SWING_ID
        | PARAM_WARP_MOTION_ID
        | PARAM_DUCKING_ID
        | PARAM_ENERGY_CEILING_ID
        | PARAM_MOD_A_DEPTH_ID
        | PARAM_MOD_B_DEPTH_ID => write!(writer, "{:.0}%", value * 100.0),
        PARAM_PULL_RATE_ID | PARAM_MOD_A_RATE_HZ_ID | PARAM_MOD_B_RATE_HZ_ID => {
            write!(writer, "{value:.2} Hz")
        }
        PARAM_PULL_SHAPE_ID => write!(writer, "{}", PullShape::from_value(value as f32).label()),
        PARAM_TIME_MODE_ID => write!(writer, "{}", TimeMode::from_value(value as f32).label()),
        PARAM_PULL_DIVISION_ID | PARAM_MOD_A_DIVISION_ID | PARAM_MOD_B_DIVISION_ID => {
            write!(writer, "{}", PullDivision::from_value(value as f32).label())
        }
        PARAM_PULL_QUANTIZE_ID => {
            write!(writer, "{}", PullQuantize::from_value(value as f32).label())
        }
        PARAM_WARP_COLOR_ID => write!(writer, "{}", WarpColor::from_value(value as f32).label()),
        PARAM_CLEAN_DIRTY_ID => {
            write!(
                writer,
                "{}",
                CharacterMode::from_value(value as f32).label()
            )
        }
        PARAM_MOD_A_SHAPE_ID | PARAM_MOD_B_SHAPE_ID => {
            write!(
                writer,
                "{}",
                ModSourceShape::from_value(value as f32).label()
            )
        }
        PARAM_MOD_A_RATE_MODE_ID | PARAM_MOD_B_RATE_MODE_ID => {
            write!(writer, "{}", ModRateMode::from_value(value as f32).label())
        }
        PARAM_HOLD_ID
        | PARAM_AIR_COMP_ID
        | PARAM_PULL_TRIGGER_ID
        | PARAM_PULL_LATCH_ID
        | PARAM_MOD_RUN_ID => {
            if value >= 0.5 {
                write!(writer, "On")
            } else {
                write!(writer, "Off")
            }
        }
        PARAM_PULL_DIRECTION_ID => {
            let bipolar = value as f32 * 2.0 - 1.0;
            write!(writer, "{bipolar:+.2}")
        }
        PARAM_OUTPUT_TRIM_DB_ID => write!(writer, "{value:+.1} dB"),
        PARAM_MOD_A_TO_TENSION_ID
        | PARAM_MOD_A_TO_DIRECTION_ID
        | PARAM_MOD_A_TO_GRAIN_ID
        | PARAM_MOD_A_TO_WIDTH_ID
        | PARAM_MOD_A_TO_WARP_MOTION_ID
        | PARAM_MOD_A_TO_FEEDBACK_ID
        | PARAM_MOD_B_TO_TENSION_ID
        | PARAM_MOD_B_TO_DIRECTION_ID
        | PARAM_MOD_B_TO_GRAIN_ID
        | PARAM_MOD_B_TO_WIDTH_ID
        | PARAM_MOD_B_TO_WARP_MOTION_ID
        | PARAM_MOD_B_TO_FEEDBACK_ID => write!(writer, "{value:+.2}"),
        _ => write!(writer, "{value:.2}"),
    }
}

/// Parse host text input into a parameter value.
pub(crate) fn text_to_value(param_id: ClapId, text: &CStr) -> Option<f64> {
    let raw = text.to_str().ok()?.trim();

    match param_id {
        PARAM_PULL_SHAPE_ID => return PullShape::parse(raw).map(|shape| shape.as_value() as f64),
        PARAM_TIME_MODE_ID => return TimeMode::parse(raw).map(|mode| mode.as_value() as f64),
        PARAM_PULL_DIVISION_ID | PARAM_MOD_A_DIVISION_ID | PARAM_MOD_B_DIVISION_ID => {
            return PullDivision::parse(raw).map(|division| division.as_value() as f64);
        }
        PARAM_PULL_QUANTIZE_ID => {
            return PullQuantize::parse(raw).map(|quantize| quantize.as_value() as f64);
        }
        PARAM_WARP_COLOR_ID => return WarpColor::parse(raw).map(|color| color.as_value() as f64),
        PARAM_CLEAN_DIRTY_ID => {
            return CharacterMode::parse(raw).map(|mode| mode.as_value() as f64);
        }
        PARAM_MOD_A_SHAPE_ID | PARAM_MOD_B_SHAPE_ID => {
            return ModSourceShape::parse(raw).map(|shape| shape.as_value() as f64);
        }
        PARAM_MOD_A_RATE_MODE_ID | PARAM_MOD_B_RATE_MODE_ID => {
            return ModRateMode::parse(raw).map(|mode| mode.as_value() as f64);
        }
        PARAM_HOLD_ID
        | PARAM_AIR_COMP_ID
        | PARAM_PULL_TRIGGER_ID
        | PARAM_PULL_LATCH_ID
        | PARAM_MOD_RUN_ID => {
            return parse_toggle(raw).map(|enabled| enabled as u8 as f64);
        }
        _ => {}
    }

    let numeric = raw
        .trim_end_matches('%')
        .trim_end_matches("hz")
        .trim_end_matches("Hz")
        .trim_end_matches("db")
        .trim_end_matches("dB")
        .trim()
        .parse::<f64>()
        .ok()?;

    let def = PARAM_DEFS.iter().find(|def| def.id == param_id)?;
    if raw.contains('%') {
        return Some((numeric / 100.0).clamp(def.min_value, def.max_value));
    }

    Some(numeric.clamp(def.min_value, def.max_value))
}

/// Parameter id for the Tension macro.
pub(crate) const PARAM_TENSION_ID: ClapId = ClapId::new(1);
/// Parameter id for pull rate (Hz).
pub(crate) const PARAM_PULL_RATE_ID: ClapId = ClapId::new(2);
/// Parameter id for pull shape selection.
pub(crate) const PARAM_PULL_SHAPE_ID: ClapId = ClapId::new(3);
/// Parameter id for legacy hold/suspend behavior.
pub(crate) const PARAM_HOLD_ID: ClapId = ClapId::new(4);
/// Parameter id for grain/continuity macro.
pub(crate) const PARAM_GRAIN_CONTINUITY_ID: ClapId = ClapId::new(5);
/// Parameter id for pitch coupling amount.
pub(crate) const PARAM_PITCH_COUPLING_ID: ClapId = ClapId::new(6);
/// Parameter id for stereo width.
pub(crate) const PARAM_WIDTH_ID: ClapId = ClapId::new(7);
/// Parameter id for space diffusion.
pub(crate) const PARAM_DIFFUSION_ID: ClapId = ClapId::new(8);
/// Parameter id for air damping amount.
pub(crate) const PARAM_AIR_DAMPING_ID: ClapId = ClapId::new(9);
/// Parameter id for air damping compensation toggle.
pub(crate) const PARAM_AIR_COMP_ID: ClapId = ClapId::new(10);
/// Parameter id for pull direction map axis.
pub(crate) const PARAM_PULL_DIRECTION_ID: ClapId = ClapId::new(11);
/// Parameter id for elasticity (map Y axis).
pub(crate) const PARAM_ELASTICITY_ID: ClapId = ClapId::new(12);
/// Parameter id for momentary pull trigger.
pub(crate) const PARAM_PULL_TRIGGER_ID: ClapId = ClapId::new(13);
/// Parameter id for release rebound amount.
pub(crate) const PARAM_REBOUND_ID: ClapId = ClapId::new(14);
/// Parameter id for character mode (legacy clean/dirty id).
pub(crate) const PARAM_CLEAN_DIRTY_ID: ClapId = ClapId::new(15);
/// Parameter id for controlled feedback amount.
pub(crate) const PARAM_FEEDBACK_ID: ClapId = ClapId::new(16);
/// Parameter id for free-vs-sync timing mode.
pub(crate) const PARAM_TIME_MODE_ID: ClapId = ClapId::new(17);
/// Parameter id for synced pull division.
pub(crate) const PARAM_PULL_DIVISION_ID: ClapId = ClapId::new(18);
/// Parameter id for timing swing amount.
pub(crate) const PARAM_SWING_ID: ClapId = ClapId::new(19);
/// Parameter id for pull latch toggle.
pub(crate) const PARAM_PULL_LATCH_ID: ClapId = ClapId::new(20);
/// Parameter id for pull trigger quantization.
pub(crate) const PARAM_PULL_QUANTIZE_ID: ClapId = ClapId::new(21);
/// Parameter id for warp spectral color mode.
pub(crate) const PARAM_WARP_COLOR_ID: ClapId = ClapId::new(22);
/// Parameter id for warp motion intensity.
pub(crate) const PARAM_WARP_MOTION_ID: ClapId = ClapId::new(23);
/// Parameter id for feedback ducking amount.
pub(crate) const PARAM_DUCKING_ID: ClapId = ClapId::new(24);
/// Parameter id for output trim in decibels.
pub(crate) const PARAM_OUTPUT_TRIM_DB_ID: ClapId = ClapId::new(25);
/// Parameter id for mod matrix run toggle.
pub(crate) const PARAM_MOD_RUN_ID: ClapId = ClapId::new(26);
/// Parameter id for modulation source A shape.
pub(crate) const PARAM_MOD_A_SHAPE_ID: ClapId = ClapId::new(27);
/// Parameter id for modulation source A rate mode.
pub(crate) const PARAM_MOD_A_RATE_MODE_ID: ClapId = ClapId::new(28);
/// Parameter id for modulation source A free rate.
pub(crate) const PARAM_MOD_A_RATE_HZ_ID: ClapId = ClapId::new(29);
/// Parameter id for modulation source A sync division.
pub(crate) const PARAM_MOD_A_DIVISION_ID: ClapId = ClapId::new(30);
/// Parameter id for modulation source A depth.
pub(crate) const PARAM_MOD_A_DEPTH_ID: ClapId = ClapId::new(31);
/// Parameter id for modulation source B shape.
pub(crate) const PARAM_MOD_B_SHAPE_ID: ClapId = ClapId::new(32);
/// Parameter id for modulation source B rate mode.
pub(crate) const PARAM_MOD_B_RATE_MODE_ID: ClapId = ClapId::new(33);
/// Parameter id for modulation source B free rate.
pub(crate) const PARAM_MOD_B_RATE_HZ_ID: ClapId = ClapId::new(34);
/// Parameter id for modulation source B sync division.
pub(crate) const PARAM_MOD_B_DIVISION_ID: ClapId = ClapId::new(35);
/// Parameter id for modulation source B depth.
pub(crate) const PARAM_MOD_B_DEPTH_ID: ClapId = ClapId::new(36);
/// Parameter id for route depth from source A to Tension.
pub(crate) const PARAM_MOD_A_TO_TENSION_ID: ClapId = ClapId::new(37);
/// Parameter id for route depth from source A to Direction.
pub(crate) const PARAM_MOD_A_TO_DIRECTION_ID: ClapId = ClapId::new(38);
/// Parameter id for route depth from source A to Grain.
pub(crate) const PARAM_MOD_A_TO_GRAIN_ID: ClapId = ClapId::new(39);
/// Parameter id for route depth from source A to Width.
pub(crate) const PARAM_MOD_A_TO_WIDTH_ID: ClapId = ClapId::new(40);
/// Parameter id for route depth from source A to Warp Motion.
pub(crate) const PARAM_MOD_A_TO_WARP_MOTION_ID: ClapId = ClapId::new(41);
/// Parameter id for route depth from source A to Feedback.
pub(crate) const PARAM_MOD_A_TO_FEEDBACK_ID: ClapId = ClapId::new(42);
/// Parameter id for route depth from source B to Tension.
pub(crate) const PARAM_MOD_B_TO_TENSION_ID: ClapId = ClapId::new(43);
/// Parameter id for route depth from source B to Direction.
pub(crate) const PARAM_MOD_B_TO_DIRECTION_ID: ClapId = ClapId::new(44);
/// Parameter id for route depth from source B to Grain.
pub(crate) const PARAM_MOD_B_TO_GRAIN_ID: ClapId = ClapId::new(45);
/// Parameter id for route depth from source B to Width.
pub(crate) const PARAM_MOD_B_TO_WIDTH_ID: ClapId = ClapId::new(46);
/// Parameter id for route depth from source B to Warp Motion.
pub(crate) const PARAM_MOD_B_TO_WARP_MOTION_ID: ClapId = ClapId::new(47);
/// Parameter id for route depth from source B to Feedback.
pub(crate) const PARAM_MOD_B_TO_FEEDBACK_ID: ClapId = ClapId::new(48);
/// Parameter id for cycle tension-bias macro.
pub(crate) const PARAM_TENSION_BIAS_ID: ClapId = ClapId::new(49);
/// Parameter id for release snap contour amount.
pub(crate) const PARAM_RELEASE_SNAP_ID: ClapId = ClapId::new(50);
/// Parameter id for soft energy ceiling amount.
pub(crate) const PARAM_ENERGY_CEILING_ID: ClapId = ClapId::new(51);

/// Pull-shape labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const PULL_SHAPE_LABELS: [&str; 5] = ["Linear", "Rubber", "Ratchet", "Wave", "Pulse"];
/// Time-mode labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const TIME_MODE_LABELS: [&str; 2] = ["Free Hz", "Sync Div"];
/// Pull-division labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const PULL_DIVISION_LABELS: [&str; 8] = [
    "1/16", "1/8T", "1/8", "1/4T", "1/4", "1/2", "1 Bar", "2 Bar",
];
/// Pull-quantize labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const PULL_QUANTIZE_LABELS: [&str; 4] = ["None", "1/16", "1/8", "1/4"];
/// Warp-color labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const WARP_COLOR_LABELS: [&str; 3] = ["Neutral", "Dark Drag", "Bright Shear"];
/// Character labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const CHARACTER_LABELS: [&str; 3] = ["Clean", "Dirty", "Crush"];
/// Mod source shape labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const MOD_SOURCE_SHAPE_LABELS: [&str; 4] =
    ["Sine", "Triangle", "Random Walk", "Envelope"];
/// Mod rate mode labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const MOD_RATE_MODE_LABELS: [&str; 2] = ["Free Hz", "Sync Div"];

#[derive(Copy, Clone)]
struct ParamDef {
    id: ClapId,
    name: &'static [u8],
    module: &'static [u8],
    min_value: f64,
    max_value: f64,
    default_value: f64,
    flags: u32,
}

impl ParamDef {
    fn to_spec(self) -> toybox::clap::params::ParamSpec<'static> {
        let flags = ParamInfoFlags::from_bits_truncate(self.flags);
        let mut builder = ParamBuilder::new(self.id, self.name, self.module)
            .range(self.min_value, self.max_value)
            .default(self.default_value);
        if flags.contains(ParamInfoFlags::IS_AUTOMATABLE) {
            builder = builder.automatable();
        }
        if flags.contains(ParamInfoFlags::IS_STEPPED) {
            builder = builder.stepped();
        }
        if flags.contains(ParamInfoFlags::IS_ENUM) {
            builder = builder.enumerated();
        }
        builder.build()
    }
}

const AUTO: u32 = ParamInfoFlags::IS_AUTOMATABLE.bits();
const TOGGLE: u32 = AUTO | ParamInfoFlags::IS_STEPPED.bits() | ParamInfoFlags::IS_ENUM.bits();

const PARAM_DEFS: &[ParamDef] = &[
    ParamDef {
        id: PARAM_TENSION_ID,
        name: b"Tension",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.5,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_PULL_RATE_ID,
        name: b"Pull Rate",
        module: b"Perform",
        min_value: 0.02,
        max_value: 4.0,
        default_value: 0.35,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_PULL_SHAPE_ID,
        name: b"Pull Shape",
        module: b"Perform",
        min_value: 0.0,
        max_value: 4.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_HOLD_ID,
        name: b"Hold",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_GRAIN_CONTINUITY_ID,
        name: b"Grain",
        module: b"Tone",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.28,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_PITCH_COUPLING_ID,
        name: b"Pitch Coupling",
        module: b"Tone",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.2,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_WIDTH_ID,
        name: b"Width",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.6,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_DIFFUSION_ID,
        name: b"Diffusion",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.55,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_AIR_DAMPING_ID,
        name: b"Air Damping",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.35,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_AIR_COMP_ID,
        name: b"Air Comp",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_PULL_DIRECTION_ID,
        name: b"Direction",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.5,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_ELASTICITY_ID,
        name: b"Elasticity",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.65,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_PULL_TRIGGER_ID,
        name: b"Pull",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_REBOUND_ID,
        name: b"Rebound",
        module: b"Perform",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.55,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_CLEAN_DIRTY_ID,
        name: b"Character",
        module: b"Tone",
        min_value: 0.0,
        max_value: 2.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_FEEDBACK_ID,
        name: b"Feedback",
        module: b"Space",
        min_value: 0.0,
        max_value: 0.7,
        default_value: 0.12,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_TIME_MODE_ID,
        name: b"Time Mode",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_PULL_DIVISION_ID,
        name: b"Pull Division",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 7.0,
        default_value: 4.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_SWING_ID,
        name: b"Swing",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_PULL_LATCH_ID,
        name: b"Pull Latch",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_PULL_QUANTIZE_ID,
        name: b"Pull Quant",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 3.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_WARP_COLOR_ID,
        name: b"Warp Color",
        module: b"Tone",
        min_value: 0.0,
        max_value: 2.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_WARP_MOTION_ID,
        name: b"Warp Motion",
        module: b"Tone",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.35,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_DUCKING_ID,
        name: b"Ducking",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_OUTPUT_TRIM_DB_ID,
        name: b"Output Trim",
        module: b"Space",
        min_value: -12.0,
        max_value: 6.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_RUN_ID,
        name: b"Mod Run",
        module: b"Mod",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_A_SHAPE_ID,
        name: b"Mod A Shape",
        module: b"Mod",
        min_value: 0.0,
        max_value: 3.0,
        default_value: 0.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_A_RATE_MODE_ID,
        name: b"Mod A Rate Mode",
        module: b"Mod",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_A_RATE_HZ_ID,
        name: b"Mod A Rate",
        module: b"Mod",
        min_value: 0.01,
        max_value: 4.0,
        default_value: 0.18,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_DIVISION_ID,
        name: b"Mod A Div",
        module: b"Mod",
        min_value: 0.0,
        max_value: 7.0,
        default_value: 5.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_A_DEPTH_ID,
        name: b"Mod A Depth",
        module: b"Mod",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.22,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_SHAPE_ID,
        name: b"Mod B Shape",
        module: b"Mod",
        min_value: 0.0,
        max_value: 3.0,
        default_value: 2.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_B_RATE_MODE_ID,
        name: b"Mod B Rate Mode",
        module: b"Mod",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_B_RATE_HZ_ID,
        name: b"Mod B Rate",
        module: b"Mod",
        min_value: 0.01,
        max_value: 4.0,
        default_value: 0.09,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_DIVISION_ID,
        name: b"Mod B Div",
        module: b"Mod",
        min_value: 0.0,
        max_value: 7.0,
        default_value: 6.0,
        flags: TOGGLE,
    },
    ParamDef {
        id: PARAM_MOD_B_DEPTH_ID,
        name: b"Mod B Depth",
        module: b"Mod",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.2,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_TENSION_ID,
        name: b"A>Tension",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.35,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_DIRECTION_ID,
        name: b"A>Direction",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.25,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_GRAIN_ID,
        name: b"A>Grain",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_WIDTH_ID,
        name: b"A>Width",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_WARP_MOTION_ID,
        name: b"A>Warp Motion",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_A_TO_FEEDBACK_ID,
        name: b"A>Feedback",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_TENSION_ID,
        name: b"B>Tension",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_DIRECTION_ID,
        name: b"B>Direction",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_GRAIN_ID,
        name: b"B>Grain",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.25,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_WIDTH_ID,
        name: b"B>Width",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.18,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_WARP_MOTION_ID,
        name: b"B>Warp Motion",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.2,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_MOD_B_TO_FEEDBACK_ID,
        name: b"B>Feedback",
        module: b"Mod Matrix",
        min_value: -1.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_TENSION_BIAS_ID,
        name: b"Tension Bias",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.5,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_RELEASE_SNAP_ID,
        name: b"Release Snap",
        module: b"Rhythm",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.35,
        flags: AUTO,
    },
    ParamDef {
        id: PARAM_ENERGY_CEILING_ID,
        name: b"Energy Ceiling",
        module: b"Safety",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.7,
        flags: AUTO,
    },
];

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.clamp(min, max)
}

fn bool_to_u32(value: bool) -> u32 {
    if value { 1 } else { 0 }
}

fn u32_to_bool(value: u32) -> bool {
    value != 0
}

fn parse_toggle(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// An atomic `f32` backed by `AtomicU32`.
#[derive(Default)]
struct AtomicF32 {
    value: AtomicU32,
}

impl AtomicF32 {
    fn new(value: f32) -> Self {
        Self {
            value: AtomicU32::new(u32::from_ne_bytes(value.to_ne_bytes())),
        }
    }

    fn store(&self, value: f32) {
        self.value
            .store(u32::from_ne_bytes(value.to_ne_bytes()), Ordering::Relaxed);
    }

    fn load(&self) -> f32 {
        f32::from_ne_bytes(self.value.load(Ordering::Relaxed).to_ne_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CharacterMode, ModRateMode, ModSourceShape, PullDivision, PullQuantize, PullShape,
        TimeMode, WarpColor, parse_toggle,
    };

    #[test]
    fn pull_shape_parse_handles_names_and_indexes() {
        assert_eq!(PullShape::parse("linear"), Some(PullShape::Linear));
        assert_eq!(PullShape::parse("2"), Some(PullShape::Ratchet));
        assert_eq!(PullShape::parse("wave"), Some(PullShape::Wave));
        assert_eq!(PullShape::parse("pulse"), Some(PullShape::Pulse));
        assert_eq!(PullShape::parse("bad"), None);
    }

    #[test]
    fn toggle_parser_handles_common_variants() {
        assert_eq!(parse_toggle("on"), Some(true));
        assert_eq!(parse_toggle("false"), Some(false));
        assert_eq!(parse_toggle("unknown"), None);
    }

    #[test]
    fn enum_parsers_cover_core_labels() {
        assert_eq!(TimeMode::parse("sync"), Some(TimeMode::SyncDivision));
        assert_eq!(PullDivision::parse("1/4"), Some(PullDivision::Div1_4));
        assert_eq!(PullQuantize::parse("1/8"), Some(PullQuantize::Div1_8));
        assert_eq!(WarpColor::parse("dark drag"), Some(WarpColor::DarkDrag));
        assert_eq!(CharacterMode::parse("crush"), Some(CharacterMode::Crush));
        assert_eq!(ModSourceShape::parse("env"), Some(ModSourceShape::Envelope));
        assert_eq!(ModRateMode::parse("hz"), Some(ModRateMode::FreeHz));
    }
}
