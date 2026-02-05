//! Parameter definitions and atomic storage for the Tension Field plugin.

use std::ffi::CStr;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};

use toybox::clack_extensions::params::{ParamDisplayWriter, ParamInfoFlags, ParamInfoWriter};
use toybox::clack_plugin::prelude::ClapId;
use toybox::clap::params::ParamBuilder;

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
}

impl PullShape {
    fn from_value(value: f32) -> Self {
        match value.round() as i32 {
            1 => Self::Rubber,
            2 => Self::Ratchet,
            3 => Self::Wave,
            _ => Self::Linear,
        }
    }

    fn as_value(self) -> f32 {
        match self {
            Self::Linear => 0.0,
            Self::Rubber => 1.0,
            Self::Ratchet => 2.0,
            Self::Wave => 3.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Rubber => "Rubber",
            Self::Ratchet => "Ratchet",
            Self::Wave => "Wave",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "linear" => Some(Self::Linear),
            "1" | "rubber" => Some(Self::Rubber),
            "2" | "ratchet" => Some(Self::Ratchet),
            "3" | "wave" => Some(Self::Wave),
            _ => None,
        }
    }
}

/// Snapshot of all parameters used by the DSP engine.
#[derive(Debug, Copy, Clone)]
pub(crate) struct TensionFieldSettings {
    /// Overall stretching force.
    pub tension: f32,
    /// Gesture rate in Hertz.
    pub pull_rate_hz: f32,
    /// Pull profile mode.
    pub pull_shape: PullShape,
    /// Frozen gesture state toggle.
    pub hold: bool,
    /// Continuity-to-grain texture macro.
    pub grain_continuity: f32,
    /// Amount of pitch-following behavior.
    pub pitch_coupling: f32,
    /// Stereo decorrelation amount.
    pub width: f32,
    /// Diffusion density amount.
    pub diffusion: f32,
    /// High-frequency damping amount.
    pub air_damping: f32,
    /// High-frequency compensation toggle.
    pub air_compensation: bool,
    /// Pull direction from backward to forward.
    pub pull_direction: f32,
    /// Viscous-to-spring behavior amount.
    pub elasticity: f32,
    /// Momentary pull trigger.
    pub pull_trigger: bool,
    /// Release rebound amount.
    pub rebound: f32,
    /// Dirty character toggle.
    pub dirty: bool,
    /// Controlled feedback amount.
    pub feedback: f32,
}

/// Thread-safe parameter storage.
pub(crate) struct TensionFieldParams {
    tension: AtomicF32,
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
    clean_dirty: AtomicF32,
    feedback: AtomicF32,
}

impl TensionFieldParams {
    /// Create a new parameter store with default values.
    pub(crate) fn new() -> Self {
        Self {
            tension: AtomicF32::new(0.55),
            pull_rate_hz: AtomicF32::new(0.22),
            pull_shape: AtomicF32::new(PullShape::Rubber.as_value()),
            hold: AtomicU32::new(0),
            grain_continuity: AtomicF32::new(0.25),
            pitch_coupling: AtomicF32::new(0.18),
            width: AtomicF32::new(0.62),
            diffusion: AtomicF32::new(0.58),
            air_damping: AtomicF32::new(0.4),
            air_compensation: AtomicU32::new(1),
            pull_direction: AtomicF32::new(0.52),
            elasticity: AtomicF32::new(0.7),
            pull_trigger: AtomicU32::new(0),
            rebound: AtomicF32::new(0.62),
            clean_dirty: AtomicF32::new(0.0),
            feedback: AtomicF32::new(0.1),
        }
    }

    /// Apply a single parameter update from CLAP automation.
    pub(crate) fn set_param(&self, param_id: ClapId, value: f32) {
        match param_id {
            PARAM_TENSION_ID => self.tension.store(clamp(value, 0.0, 1.0)),
            PARAM_PULL_RATE_ID => self.pull_rate_hz.store(clamp(value, 0.02, 2.0)),
            PARAM_PULL_SHAPE_ID => self.pull_shape.store(clamp(value, 0.0, 3.0).round()),
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
            PARAM_CLEAN_DIRTY_ID => self.clean_dirty.store(clamp(value, 0.0, 1.0)),
            PARAM_FEEDBACK_ID => self.feedback.store(clamp(value, 0.0, 0.6)),
            _ => {}
        }
    }

    /// Fetch a parameter value for host reads.
    pub(crate) fn get_param(&self, param_id: ClapId) -> Option<f32> {
        match param_id {
            PARAM_TENSION_ID => Some(self.tension.load()),
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
            PARAM_CLEAN_DIRTY_ID => Some(self.clean_dirty.load()),
            PARAM_FEEDBACK_ID => Some(self.feedback.load()),
            _ => None,
        }
    }

    /// Build an immutable settings snapshot for one audio block.
    pub(crate) fn settings(&self) -> TensionFieldSettings {
        TensionFieldSettings {
            tension: self.tension.load(),
            pull_rate_hz: self.pull_rate_hz.load(),
            pull_shape: PullShape::from_value(self.pull_shape.load()),
            hold: u32_to_bool(self.hold.load(Ordering::Relaxed)),
            grain_continuity: self.grain_continuity.load(),
            pitch_coupling: self.pitch_coupling.load(),
            width: self.width.load(),
            diffusion: self.diffusion.load(),
            air_damping: self.air_damping.load(),
            air_compensation: u32_to_bool(self.air_compensation.load(Ordering::Relaxed)),
            pull_direction: self.pull_direction.load() * 2.0 - 1.0,
            elasticity: self.elasticity.load(),
            pull_trigger: u32_to_bool(self.pull_trigger.load(Ordering::Relaxed)),
            rebound: self.rebound.load(),
            dirty: self.clean_dirty.load() >= 0.5,
            feedback: self.feedback.load(),
        }
    }

    /// Read the tension value.
    #[cfg(target_os = "windows")]
    pub(crate) fn tension(&self) -> f32 {
        self.tension.load()
    }

    /// Read the pull rate in Hertz.
    #[cfg(target_os = "windows")]
    pub(crate) fn pull_rate_hz(&self) -> f32 {
        self.pull_rate_hz.load()
    }

    /// Read the pull shape as an index in `PULL_SHAPE_LABELS`.
    #[cfg(target_os = "windows")]
    pub(crate) fn pull_shape_index(&self) -> usize {
        PullShape::from_value(self.pull_shape.load()).as_value() as usize
    }

    /// Read the hold state.
    #[cfg(target_os = "windows")]
    pub(crate) fn hold(&self) -> bool {
        u32_to_bool(self.hold.load(Ordering::Relaxed))
    }

    /// Read the grain/continuity macro.
    #[cfg(target_os = "windows")]
    pub(crate) fn grain_continuity(&self) -> f32 {
        self.grain_continuity.load()
    }

    /// Read the pitch coupling amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn pitch_coupling(&self) -> f32 {
        self.pitch_coupling.load()
    }

    /// Read the width amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn width(&self) -> f32 {
        self.width.load()
    }

    /// Read the diffusion amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn diffusion(&self) -> f32 {
        self.diffusion.load()
    }

    /// Read the air damping amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn air_damping(&self) -> f32 {
        self.air_damping.load()
    }

    /// Read whether air compensation is enabled.
    #[cfg(target_os = "windows")]
    pub(crate) fn air_compensation(&self) -> bool {
        u32_to_bool(self.air_compensation.load(Ordering::Relaxed))
    }

    /// Read the pull direction value (0..1).
    #[cfg(target_os = "windows")]
    pub(crate) fn pull_direction(&self) -> f32 {
        self.pull_direction.load()
    }

    /// Read the elasticity value (0..1).
    #[cfg(target_os = "windows")]
    pub(crate) fn elasticity(&self) -> f32 {
        self.elasticity.load()
    }

    /// Read whether the pull trigger is active.
    #[cfg(target_os = "windows")]
    pub(crate) fn pull_trigger(&self) -> bool {
        u32_to_bool(self.pull_trigger.load(Ordering::Relaxed))
    }

    /// Read the rebound amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn rebound(&self) -> f32 {
        self.rebound.load()
    }

    /// Read the clean/dirty control value.
    #[cfg(target_os = "windows")]
    pub(crate) fn clean_dirty(&self) -> f32 {
        self.clean_dirty.load()
    }

    /// Read the feedback amount.
    #[cfg(target_os = "windows")]
    pub(crate) fn feedback(&self) -> f32 {
        self.feedback.load()
    }
}

/// Convert a pull-shape index to an internal shape value.
#[cfg(target_os = "windows")]
pub(crate) fn pull_shape_value_from_index(index: usize) -> f32 {
    match index {
        1 => PullShape::Rubber.as_value(),
        2 => PullShape::Ratchet.as_value(),
        3 => PullShape::Wave.as_value(),
        _ => PullShape::Linear.as_value(),
    }
}

/// Return the number of host-visible parameters.
pub(crate) fn param_count() -> u32 {
    PARAM_DEFS.len() as u32
}

/// Number of serialized parameter values stored in plugin state.
pub(crate) const STATE_VALUE_COUNT: usize = PARAM_DEFS.len();

/// Build a stable, ordered parameter snapshot for CLAP state serialization.
pub(crate) fn state_values(params: &TensionFieldParams) -> [f32; STATE_VALUE_COUNT] {
    let mut values = [0.0; STATE_VALUE_COUNT];
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
        | PARAM_GRAIN_CONTINUITY_ID
        | PARAM_PITCH_COUPLING_ID
        | PARAM_WIDTH_ID
        | PARAM_DIFFUSION_ID
        | PARAM_AIR_DAMPING_ID
        | PARAM_ELASTICITY_ID
        | PARAM_REBOUND_ID
        | PARAM_FEEDBACK_ID => write!(writer, "{:.0}%", value * 100.0),
        PARAM_PULL_RATE_ID => write!(writer, "{value:.2} Hz"),
        PARAM_PULL_SHAPE_ID => write!(writer, "{}", PullShape::from_value(value as f32).label()),
        PARAM_HOLD_ID | PARAM_AIR_COMP_ID | PARAM_PULL_TRIGGER_ID => {
            if value >= 0.5 {
                write!(writer, "On")
            } else {
                write!(writer, "Off")
            }
        }
        PARAM_CLEAN_DIRTY_ID => {
            if value >= 0.5 {
                write!(writer, "Dirty")
            } else {
                write!(writer, "Clean")
            }
        }
        PARAM_PULL_DIRECTION_ID => {
            if value >= 0.5 {
                write!(writer, "Forward")
            } else {
                write!(writer, "Backward")
            }
        }
        _ => write!(writer, "{value:.2}"),
    }
}

/// Parse host text input into a parameter value.
pub(crate) fn text_to_value(param_id: ClapId, text: &CStr) -> Option<f64> {
    let raw = text.to_str().ok()?.trim();

    match param_id {
        PARAM_PULL_SHAPE_ID => return PullShape::parse(raw).map(|shape| shape.as_value() as f64),
        PARAM_HOLD_ID | PARAM_AIR_COMP_ID | PARAM_PULL_TRIGGER_ID => {
            return parse_toggle(raw).map(|enabled| enabled as u8 as f64);
        }
        PARAM_CLEAN_DIRTY_ID => {
            let normalized = raw.to_ascii_lowercase();
            if normalized == "clean" {
                return Some(0.0);
            }
            if normalized == "dirty" {
                return Some(1.0);
            }
        }
        PARAM_PULL_DIRECTION_ID => {
            let normalized = raw.to_ascii_lowercase();
            if normalized == "backward" || normalized == "left" {
                return Some(0.0);
            }
            if normalized == "forward" || normalized == "right" {
                return Some(1.0);
            }
        }
        _ => {}
    }

    let numeric = raw
        .trim_end_matches('%')
        .trim_end_matches("hz")
        .trim_end_matches("Hz")
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
/// Parameter id for hold/suspend.
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
/// Parameter id for pull direction (2D map X axis).
pub(crate) const PARAM_PULL_DIRECTION_ID: ClapId = ClapId::new(11);
/// Parameter id for elasticity (2D map Y axis).
pub(crate) const PARAM_ELASTICITY_ID: ClapId = ClapId::new(12);
/// Parameter id for momentary pull trigger.
pub(crate) const PARAM_PULL_TRIGGER_ID: ClapId = ClapId::new(13);
/// Parameter id for release rebound amount.
pub(crate) const PARAM_REBOUND_ID: ClapId = ClapId::new(14);
/// Parameter id for clean/dirty character switch.
pub(crate) const PARAM_CLEAN_DIRTY_ID: ClapId = ClapId::new(15);
/// Parameter id for controlled feedback amount.
pub(crate) const PARAM_FEEDBACK_ID: ClapId = ClapId::new(16);

/// Pull-shape labels used by the editor dropdown.
#[cfg(target_os = "windows")]
pub(crate) const PULL_SHAPE_LABELS: [&str; 4] = ["Linear", "Rubber", "Ratchet", "Wave"];

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

const PARAM_DEFS: &[ParamDef] = &[
    ParamDef {
        id: PARAM_TENSION_ID,
        name: b"Tension",
        module: b"Gesture",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.55,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_PULL_RATE_ID,
        name: b"Pull Rate",
        module: b"Gesture",
        min_value: 0.02,
        max_value: 2.0,
        default_value: 0.22,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_PULL_SHAPE_ID,
        name: b"Pull Shape",
        module: b"Gesture",
        min_value: 0.0,
        max_value: 3.0,
        default_value: 1.0,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits()
            | ParamInfoFlags::IS_STEPPED.bits()
            | ParamInfoFlags::IS_ENUM.bits(),
    },
    ParamDef {
        id: PARAM_HOLD_ID,
        name: b"Hold",
        module: b"Gesture",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits()
            | ParamInfoFlags::IS_STEPPED.bits()
            | ParamInfoFlags::IS_ENUM.bits(),
    },
    ParamDef {
        id: PARAM_GRAIN_CONTINUITY_ID,
        name: b"Grain",
        module: b"Elastic",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.25,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_PITCH_COUPLING_ID,
        name: b"Pitch Coupling",
        module: b"Elastic",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.18,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_WIDTH_ID,
        name: b"Width",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.62,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_DIFFUSION_ID,
        name: b"Diffusion",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.58,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_AIR_DAMPING_ID,
        name: b"Air Damping",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.4,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_AIR_COMP_ID,
        name: b"Air Comp",
        module: b"Space",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 1.0,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits()
            | ParamInfoFlags::IS_STEPPED.bits()
            | ParamInfoFlags::IS_ENUM.bits(),
    },
    ParamDef {
        id: PARAM_PULL_DIRECTION_ID,
        name: b"Pull Direction",
        module: b"Map",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.52,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_ELASTICITY_ID,
        name: b"Elasticity",
        module: b"Map",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.7,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_PULL_TRIGGER_ID,
        name: b"Pull",
        module: b"Gesture",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits()
            | ParamInfoFlags::IS_STEPPED.bits()
            | ParamInfoFlags::IS_ENUM.bits(),
    },
    ParamDef {
        id: PARAM_REBOUND_ID,
        name: b"Rebound",
        module: b"Gesture",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.62,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
    },
    ParamDef {
        id: PARAM_CLEAN_DIRTY_ID,
        name: b"Clean Dirty",
        module: b"Character",
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits()
            | ParamInfoFlags::IS_STEPPED.bits()
            | ParamInfoFlags::IS_ENUM.bits(),
    },
    ParamDef {
        id: PARAM_FEEDBACK_ID,
        name: b"Feedback",
        module: b"Character",
        min_value: 0.0,
        max_value: 0.6,
        default_value: 0.1,
        flags: ParamInfoFlags::IS_AUTOMATABLE.bits(),
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
    use super::{PullShape, parse_toggle};

    #[test]
    fn pull_shape_parse_handles_names_and_indexes() {
        assert_eq!(PullShape::parse("linear"), Some(PullShape::Linear));
        assert_eq!(PullShape::parse("2"), Some(PullShape::Ratchet));
        assert_eq!(PullShape::parse("wave"), Some(PullShape::Wave));
        assert_eq!(PullShape::parse("bad"), None);
    }

    #[test]
    fn toggle_parser_handles_common_variants() {
        assert_eq!(parse_toggle("on"), Some(true));
        assert_eq!(parse_toggle("false"), Some(false));
        assert_eq!(parse_toggle("unknown"), None);
    }
}
