//! Gesture synthesis for pull timing, latch, and quantized trigger behavior.

use std::f32::consts::TAU;

use crate::clock::ClockFrame;
use crate::params::{PullDivision, PullQuantize, PullShape, TimeMode};

/// Per-sample control inputs for the gesture engine.
#[derive(Debug, Copy, Clone)]
pub(crate) struct GestureInput {
    /// Base tension amount.
    pub tension: f32,
    /// Free-vs-sync timing mode.
    pub time_mode: TimeMode,
    /// Free-running rate in Hertz.
    pub pull_rate_hz: f32,
    /// Synced rate division.
    pub pull_division: PullDivision,
    /// Swing amount for synced timing.
    pub swing: f32,
    /// Pull waveform shape.
    pub pull_shape: PullShape,
    /// Momentary pull trigger.
    pub pull_trigger: bool,
    /// Latching pull mode toggle.
    pub pull_latch: bool,
    /// Quantization grid for trigger launches.
    pub pull_quantize: PullQuantize,
    /// Rebound amount controlling release shape.
    pub rebound: f32,
    /// Direction bias from backward to forward.
    pub pull_direction: f32,
    /// Viscous-to-spring response amount.
    pub elasticity: f32,
}

/// Per-sample gesture frame used by downstream DSP stages.
#[derive(Copy, Clone, Debug)]
pub(crate) struct GestureFrame {
    /// Elastic-buffer target delay length.
    pub delay_samples: f32,
    /// Signed motion velocity used for pitch coupling.
    pub velocity: f32,
    /// 0..1 tension drive amount.
    pub tension_drive: f32,
    /// Drift phase increment used by warp motion.
    pub drift_phase_inc: f32,
}

/// Runtime state for pull envelopes and timing.
#[derive(Default)]
pub(crate) struct GestureEngine {
    free_phase: f32,
    pull_env: f32,
    random_walk: f32,
    previous_direction: f32,
    was_pull_pressed: bool,
    latched_active: bool,
    pending_quantized_trigger: bool,
    one_shot_samples: usize,
    previous_beat_position: Option<f64>,
    rng_state: u32,
}

impl GestureEngine {
    /// Generate one gesture frame at the current sample.
    pub(crate) fn next(
        &mut self,
        input: GestureInput,
        sample_rate: f32,
        clock: ClockFrame,
    ) -> GestureFrame {
        if self.rng_state == 0 {
            self.rng_state = 0x9E37_79B9;
        }

        let rising_edge = input.pull_trigger && !self.was_pull_pressed;
        self.was_pull_pressed = input.pull_trigger;

        if !input.pull_latch {
            self.latched_active = false;
        }

        if rising_edge {
            if input.pull_latch {
                self.latched_active = true;
            }

            if input.pull_quantize.beats().is_none() || !clock.is_playing {
                self.start_pull(sample_rate);
            } else {
                self.pending_quantized_trigger = true;
            }
        }

        if self.pending_quantized_trigger {
            if let Some(grid_beats) = input.pull_quantize.beats() {
                if self.crossed_quantize_boundary(clock.beat_position, grid_beats as f64) {
                    self.start_pull(sample_rate);
                    self.pending_quantized_trigger = false;
                }
            } else {
                self.start_pull(sample_rate);
                self.pending_quantized_trigger = false;
            }
        }

        let phase = match input.time_mode {
            TimeMode::FreeHz => {
                let increment = (input.pull_rate_hz / sample_rate.max(1.0)).clamp(0.000_01, 0.25);
                self.free_phase = (self.free_phase + increment).fract();
                self.free_phase
            }
            TimeMode::SyncDivision => clock.phase_for_division(input.pull_division, input.swing),
        };

        let envelope_target: f32 = if input.pull_latch {
            if self.latched_active { 1.0 } else { 0.0 }
        } else if input.pull_trigger {
            1.0
        } else {
            0.0
        };

        let one_shot_active = self.one_shot_samples > 0;
        if self.one_shot_samples > 0 {
            self.one_shot_samples -= 1;
        }

        let target = envelope_target.max(if one_shot_active { 1.0 } else { 0.0 });
        let attack = 0.006 + input.elasticity * 0.028;
        let release = 0.0009 + input.rebound * 0.028;
        let smoothing = if target > self.pull_env {
            attack
        } else {
            release
        };
        self.pull_env += (target - self.pull_env) * smoothing;

        let walk_amount = 0.0012 + input.elasticity * 0.005;
        self.random_walk =
            (self.random_walk + next_signed(&mut self.rng_state) * walk_amount).clamp(-1.0, 1.0);

        let shape_value = evaluate_shape(input.pull_shape, phase);
        let motion = shape_value * (0.3 + self.pull_env * 0.7)
            + self.random_walk * (0.04 + input.elasticity * 0.1);

        let directional = (motion * 0.7 + input.pull_direction * 0.65).clamp(-1.0, 1.0);
        let velocity = directional - self.previous_direction;
        self.previous_direction = directional;

        let tension_drive = (input.tension * (0.2 + directional.abs() * 0.8)).clamp(0.0, 1.0);
        let center_delay = sample_rate * (0.05 + input.tension * 0.2);
        let delay_swing = sample_rate * (0.004 + input.elasticity * 0.075);
        let delay_samples = (center_delay + directional * delay_swing).max(12.0);

        let drift_phase_inc =
            (0.0002 + velocity.abs() * 0.018 + tension_drive * 0.008).clamp(0.0001, 0.08);

        self.previous_beat_position = Some(clock.beat_position);

        GestureFrame {
            delay_samples,
            velocity,
            tension_drive,
            drift_phase_inc,
        }
    }

    fn start_pull(&mut self, sample_rate: f32) {
        self.one_shot_samples = (sample_rate * 0.11).round() as usize;
    }

    fn crossed_quantize_boundary(&self, beat_position: f64, grid_beats: f64) -> bool {
        let previous = self.previous_beat_position.unwrap_or(beat_position);
        let prev_index = (previous / grid_beats).floor();
        let current_index = (beat_position / grid_beats).floor();
        current_index > prev_index
    }
}

fn evaluate_shape(shape: PullShape, phase: f32) -> f32 {
    let phase = phase.fract();
    match shape {
        PullShape::Linear => phase * 2.0 - 1.0,
        PullShape::Rubber => {
            let s = (phase * TAU).sin();
            s.signum() * s.abs().powf(0.6)
        }
        PullShape::Ratchet => {
            let steps = 6.0;
            let stepped = ((phase * steps).floor() / (steps - 1.0)) * 2.0 - 1.0;
            let softener = (phase * TAU).sin() * 0.18;
            (stepped * 0.86 + softener).clamp(-1.0, 1.0)
        }
        PullShape::Wave => (phase * TAU).sin(),
        PullShape::Pulse => {
            if phase < 0.2 {
                1.0
            } else if phase < 0.45 {
                -0.2
            } else if phase < 0.65 {
                0.6
            } else {
                -1.0
            }
        }
    }
}

fn next_signed(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    ((x as f32 / u32::MAX as f32) * 2.0) - 1.0
}

#[cfg(test)]
mod tests {
    use super::{GestureEngine, GestureInput, evaluate_shape};
    use crate::clock::ClockFrame;
    use crate::params::{PullDivision, PullQuantize, PullShape, TimeMode};

    fn base_input() -> GestureInput {
        GestureInput {
            tension: 0.6,
            time_mode: TimeMode::SyncDivision,
            pull_rate_hz: 0.25,
            pull_division: PullDivision::Div1_4,
            swing: 0.0,
            pull_shape: PullShape::Rubber,
            pull_trigger: false,
            pull_latch: false,
            pull_quantize: PullQuantize::None,
            rebound: 0.5,
            pull_direction: 0.2,
            elasticity: 0.7,
        }
    }

    #[test]
    fn shape_values_stay_in_range() {
        for shape in [
            PullShape::Linear,
            PullShape::Rubber,
            PullShape::Ratchet,
            PullShape::Wave,
            PullShape::Pulse,
        ] {
            for i in 0..64 {
                let phase = i as f32 / 64.0;
                let value = evaluate_shape(shape, phase);
                assert!((-1.01..=1.01).contains(&value));
            }
        }
    }

    #[test]
    fn latch_keeps_envelope_active_after_trigger_release() {
        let mut engine = GestureEngine::default();
        let mut input = base_input();
        input.pull_latch = true;
        input.pull_trigger = true;

        let _ = engine.next(
            input,
            48_000.0,
            ClockFrame {
                beat_position: 0.0,
                is_playing: true,
            },
        );

        input.pull_trigger = false;
        let frame = engine.next(
            input,
            48_000.0,
            ClockFrame {
                beat_position: 0.01,
                is_playing: true,
            },
        );

        assert!(frame.tension_drive > 0.0);
    }
}
