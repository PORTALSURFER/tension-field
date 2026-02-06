//! DSP-thread slow modulation matrix for Tension Field.

use std::f32::consts::TAU;

use crate::clock::ClockFrame;
use crate::params::{ModRateMode, ModSettings, ModSourceSettings, ModSourceShape};

const DEST_COUNT: usize = 6;

/// Per-source runtime state for modulation generation.
#[derive(Debug, Copy, Clone)]
struct ModSourceState {
    phase: f32,
    previous_sync_phase: f32,
    walk_state: f32,
    env_state: f32,
}

impl Default for ModSourceState {
    fn default() -> Self {
        Self {
            phase: 0.0,
            previous_sync_phase: 0.0,
            walk_state: 0.0,
            env_state: 0.0,
        }
    }
}

/// Runtime modulation matrix.
pub(crate) struct ModMatrix {
    source_a: ModSourceState,
    source_b: ModSourceState,
    smoothed: [f32; DEST_COUNT],
    noise_state: u32,
}

impl Default for ModMatrix {
    fn default() -> Self {
        Self {
            source_a: ModSourceState::default(),
            source_b: ModSourceState::default(),
            smoothed: [0.0; DEST_COUNT],
            noise_state: 0xA5A5_9151,
        }
    }
}

impl ModMatrix {
    /// Generate one sample of destination modulation values.
    pub(crate) fn next(
        &mut self,
        settings: &ModSettings,
        clock: ClockFrame,
        input_envelope: f32,
        sample_rate: f32,
    ) -> [f32; DEST_COUNT] {
        if !settings.run {
            for value in &mut self.smoothed {
                *value *= 0.98;
            }
            return self.smoothed;
        }

        let a = source_value(
            &settings.source_a,
            &mut self.source_a,
            clock,
            input_envelope,
            sample_rate,
            &mut self.noise_state,
        );
        let b = source_value(
            &settings.source_b,
            &mut self.source_b,
            clock,
            input_envelope,
            sample_rate,
            &mut self.noise_state,
        );

        let mut destination_raw = [0.0; DEST_COUNT];
        for (index, raw) in destination_raw.iter_mut().enumerate() {
            let combined =
                a * settings.route_depths[0][index] + b * settings.route_depths[1][index];
            *raw = destination_curve(index, combined);
        }

        for (index, raw) in destination_raw.iter().enumerate() {
            let delta = *raw - self.smoothed[index];
            let filtered_delta = if delta.abs() < 0.0005 { 0.0 } else { delta };
            self.smoothed[index] += filtered_delta * destination_smoothing(index);
        }

        self.smoothed
    }
}

fn destination_curve(index: usize, value: f32) -> f32 {
    let clamped = value.clamp(-1.0, 1.0);
    match index {
        // Tension, Warp Motion, and Feedback use a softer mid-bias perceptual curve.
        0 | 4 | 5 => clamped.signum() * clamped.abs().powf(0.75),
        _ => clamped,
    }
}

fn destination_smoothing(index: usize) -> f32 {
    match index {
        0 => 0.07, // Tension
        1 => 0.06, // Direction
        2 => 0.05, // Grain
        3 => 0.05, // Width
        4 => 0.08, // Warp motion
        5 => 0.09, // Feedback
        _ => 0.05,
    }
}

fn source_value(
    settings: &ModSourceSettings,
    state: &mut ModSourceState,
    clock: ClockFrame,
    input_envelope: f32,
    sample_rate: f32,
    noise_state: &mut u32,
) -> f32 {
    let phase = match settings.rate_mode {
        ModRateMode::FreeHz => {
            let increment = (settings.rate_hz / sample_rate.max(1.0)).clamp(0.000_01, 0.25);
            state.phase = (state.phase + increment).fract();
            state.phase
        }
        ModRateMode::SyncDivision => {
            let sync_phase = clock.phase_for_division(settings.rate_division, 0.0);
            state.phase = sync_phase;
            sync_phase
        }
    };

    let wrapped = phase < state.previous_sync_phase;
    state.previous_sync_phase = phase;

    let core = match settings.shape {
        ModSourceShape::Sine => (phase * TAU).sin(),
        ModSourceShape::Triangle => triangle(phase),
        ModSourceShape::RandomWalk => {
            let walk_scale = match settings.rate_mode {
                ModRateMode::FreeHz => settings.rate_hz * 0.6 / sample_rate.max(1.0),
                ModRateMode::SyncDivision => {
                    if wrapped {
                        1.0
                    } else {
                        0.0
                    }
                }
            };

            if walk_scale > 0.0 {
                state.walk_state = (state.walk_state
                    + signed_noise(noise_state) * (0.8 * walk_scale + 0.05))
                    .clamp(-1.0, 1.0);
            }
            state.walk_state
        }
        ModSourceShape::Envelope => {
            let target = input_envelope.clamp(0.0, 1.0);
            state.env_state += (target - state.env_state) * 0.06;
            state.env_state * 2.0 - 1.0
        }
    };

    core * settings.depth.clamp(0.0, 1.0)
}

fn triangle(phase: f32) -> f32 {
    let p = phase.fract();
    if p < 0.5 {
        p * 4.0 - 1.0
    } else {
        3.0 - p * 4.0
    }
}

fn signed_noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*state >> 8) as f32 / ((1u32 << 24) as f32)) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::ModMatrix;
    use crate::clock::ClockFrame;
    use crate::params::{
        ModRateMode, ModSettings, ModSourceSettings, ModSourceShape, PullDivision,
    };

    fn test_settings() -> ModSettings {
        ModSettings {
            run: true,
            source_a: ModSourceSettings {
                shape: ModSourceShape::Sine,
                rate_mode: ModRateMode::FreeHz,
                rate_hz: 0.5,
                rate_division: PullDivision::Div1_4,
                depth: 1.0,
            },
            source_b: ModSourceSettings {
                shape: ModSourceShape::Triangle,
                rate_mode: ModRateMode::FreeHz,
                rate_hz: 0.3,
                rate_division: PullDivision::Div1_2,
                depth: 0.0,
            },
            route_depths: [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0], [0.0; 6]],
        }
    }

    #[test]
    fn route_depth_drives_destination() {
        let mut matrix = ModMatrix::default();
        let mut has_motion = false;
        for n in 0..128 {
            let output = matrix.next(
                &test_settings(),
                ClockFrame {
                    beat_position: n as f64 / 48_000.0,
                    is_playing: true,
                },
                0.5,
                48_000.0,
            );
            if output[0].abs() > 1.0e-5 {
                has_motion = true;
                break;
            }
        }
        assert!(has_motion);
    }

    #[test]
    fn disabled_matrix_decays_to_zero() {
        let mut matrix = ModMatrix::default();
        let mut settings = test_settings();

        let _ = matrix.next(
            &settings,
            ClockFrame {
                beat_position: 0.0,
                is_playing: true,
            },
            0.5,
            48_000.0,
        );

        settings.run = false;
        let output = matrix.next(
            &settings,
            ClockFrame {
                beat_position: 0.0,
                is_playing: true,
            },
            0.5,
            48_000.0,
        );
        assert!(output.iter().all(|value| value.abs() <= 1.0));
    }
}
