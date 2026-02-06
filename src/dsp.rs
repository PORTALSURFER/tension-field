//! Core DSP for the Tension Field effect.

use std::f32::consts::TAU;

use crate::clock::{TransportClock, TransportState};
use crate::gesture::{GestureEngine, GestureInput};
use crate::mod_matrix::ModMatrix;
use crate::params::{CharacterMode, TensionFieldSettings, WarpColor};

/// Per-block metering information exported to the GUI thread.
#[derive(Debug, Copy, Clone, Default)]
pub(crate) struct RenderReport {
    /// Input left activity (0..1).
    pub input_left: f32,
    /// Input right activity (0..1).
    pub input_right: f32,
    /// Elastic stage activity (0..1).
    pub elastic_activity: f32,
    /// Warp stage activity (0..1).
    pub warp_activity: f32,
    /// Space stage activity (0..1).
    pub space_activity: f32,
    /// Feedback path activity (0..1).
    pub feedback_activity: f32,
    /// Output left activity (0..1).
    pub output_left: f32,
    /// Output right activity (0..1).
    pub output_right: f32,
    /// Tension drive activity (0..1).
    pub tension_activity: f32,
}

/// Audio engine implementing transport-aware gestures, modulation, and signal stages.
pub(crate) struct TensionFieldEngine {
    sample_rate: f32,
    clock: TransportClock,
    pre_left: PreEmphasis,
    pre_right: PreEmphasis,
    gesture: GestureEngine,
    modulation: ModMatrix,
    elastic: ElasticBuffer,
    warp_left: SpectralWarp,
    warp_right: SpectralWarp,
    space: SpaceStage,
    feedback_left: f32,
    feedback_right: f32,
    input_env: f32,
    output_gain: f32,
}

impl TensionFieldEngine {
    /// Create a new Tension Field engine at the given sample rate.
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            clock: TransportClock::new(sample_rate),
            pre_left: PreEmphasis::default(),
            pre_right: PreEmphasis::default(),
            gesture: GestureEngine::default(),
            modulation: ModMatrix::default(),
            elastic: ElasticBuffer::new(sample_rate),
            warp_left: SpectralWarp::new(37, 73),
            warp_right: SpectralWarp::new(43, 79),
            space: SpaceStage::default(),
            feedback_left: 0.0,
            feedback_right: 0.0,
            input_env: 0.0,
            output_gain: 1.0,
        }
    }

    /// Process one stereo block in place.
    pub(crate) fn render(
        &mut self,
        settings: &TensionFieldSettings,
        left: &mut [f32],
        right: &mut [f32],
        transport: TransportState,
    ) -> RenderReport {
        let frames = left.len().min(right.len());
        if frames == 0 {
            return RenderReport::default();
        }

        let mut input_left_peak = 0.0_f32;
        let mut input_right_peak = 0.0_f32;
        let mut elastic_peak = 0.0_f32;
        let mut warp_peak = 0.0_f32;
        let mut space_peak = 0.0_f32;
        let mut feedback_peak = 0.0_f32;
        let mut output_left_peak = 0.0_f32;
        let mut output_right_peak = 0.0_f32;
        let mut tension_peak = 0.0_f32;

        let mut transport_for_sample = transport;
        for (l, r) in left.iter_mut().zip(right.iter_mut()).take(frames) {
            let in_l = *l;
            let in_r = *r;
            input_left_peak = input_left_peak.max(in_l.abs());
            input_right_peak = input_right_peak.max(in_r.abs());

            let input_abs = in_l.abs().max(in_r.abs());
            self.input_env += (input_abs - self.input_env) * (0.01 + settings.ducking * 0.08);

            let clock = self.clock.tick(transport_for_sample);
            transport_for_sample.song_pos_beats = None;

            let mod_values = self.modulation.next(
                &settings.modulation,
                clock,
                self.input_env,
                self.sample_rate,
            );

            let tension = (settings.tension + mod_values[0]).clamp(0.0, 1.0);
            let pull_direction = (settings.pull_direction + mod_values[1]).clamp(-1.0, 1.0);
            let grain = (settings.grain_continuity + mod_values[2]).clamp(0.0, 1.0);
            let width = (settings.width + mod_values[3]).clamp(0.0, 1.0);
            let warp_motion = (settings.warp_motion + mod_values[4]).clamp(0.0, 1.0);
            let feedback = (settings.feedback + mod_values[5]).clamp(0.0, 0.7);

            let gesture = self.gesture.next(
                GestureInput {
                    tension,
                    time_mode: settings.time_mode,
                    pull_rate_hz: settings.pull_rate_hz,
                    pull_division: settings.pull_division,
                    swing: settings.swing,
                    pull_shape: settings.pull_shape,
                    pull_trigger: settings.pull_trigger,
                    pull_latch: settings.pull_latch,
                    pull_quantize: settings.pull_quantize,
                    rebound: settings.rebound,
                    pull_direction,
                    elasticity: settings.elasticity,
                },
                self.sample_rate,
                clock,
            );
            tension_peak = tension_peak.max(gesture.tension_drive);

            let duck_gain = 1.0 - settings.ducking * self.input_env.clamp(0.0, 1.0) * 0.85;
            let feedback_l = self.feedback_left * feedback * duck_gain;
            let feedback_r = self.feedback_right * feedback * duck_gain;
            feedback_peak = feedback_peak.max(feedback_l.abs().max(feedback_r.abs()));

            let pre_l = self
                .pre_left
                .process(in_l + feedback_l, gesture.tension_drive, grain);
            let pre_r = self
                .pre_right
                .process(in_r + feedback_r, gesture.tension_drive, grain);

            let character_dirty = settings.character != CharacterMode::Clean;
            let (elastic_l, elastic_r) = self.elastic.process(
                pre_l,
                pre_r,
                ElasticControl {
                    delay_samples: gesture.delay_samples,
                    velocity: gesture.velocity,
                    pitch_coupling: settings.pitch_coupling,
                    grain_amount: grain,
                    elasticity: settings.elasticity,
                    dirty: character_dirty,
                },
            );
            elastic_peak =
                elastic_peak.max((elastic_l - pre_l).abs().max((elastic_r - pre_r).abs()));

            let warp_control = WarpControl {
                tension: gesture.tension_drive,
                diffusion: settings.diffusion,
                elasticity: settings.elasticity,
                air_damping: settings.air_damping,
                air_compensation: settings.air_compensation,
                drift_phase_inc: gesture.drift_phase_inc,
                warp_motion,
                color: settings.warp_color,
                character: settings.character,
            };
            let warped_l = self.warp_left.process(elastic_l, warp_control);
            let warped_r = self.warp_right.process(elastic_r, warp_control);
            warp_peak = warp_peak.max(
                (warped_l - elastic_l)
                    .abs()
                    .max((warped_r - elastic_r).abs()),
            );

            let (space_l, space_r) = self.space.process(
                warped_l,
                warped_r,
                width,
                settings.diffusion,
                character_dirty,
            );
            space_peak = space_peak.max((space_l - warped_l).abs().max((space_r - warped_r).abs()));

            self.output_gain += (db_to_gain(settings.output_trim_db) - self.output_gain) * 0.002;
            let mut out_l = space_l * self.output_gain;
            let mut out_r = space_r * self.output_gain;
            if settings.character == CharacterMode::Crush {
                out_l = crush(out_l);
                out_r = crush(out_r);
            }

            out_l = soft_clip(out_l);
            out_r = soft_clip(out_r);

            *l = out_l;
            *r = out_r;
            output_left_peak = output_left_peak.max(out_l.abs());
            output_right_peak = output_right_peak.max(out_r.abs());
            self.feedback_left = out_l;
            self.feedback_right = out_r;
        }

        RenderReport {
            input_left: meter_norm(input_left_peak),
            input_right: meter_norm(input_right_peak),
            elastic_activity: meter_norm(elastic_peak),
            warp_activity: meter_norm(warp_peak),
            space_activity: meter_norm(space_peak),
            feedback_activity: meter_norm(feedback_peak),
            output_left: meter_norm(output_left_peak),
            output_right: meter_norm(output_right_peak),
            tension_activity: tension_peak.clamp(0.0, 1.0),
        }
    }
}

#[derive(Copy, Clone)]
struct ElasticControl {
    delay_samples: f32,
    velocity: f32,
    pitch_coupling: f32,
    grain_amount: f32,
    elasticity: f32,
    dirty: bool,
}

struct ElasticBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
    write_index: usize,
    read_position: f32,
    smooth_delay: f32,
    jitter: f32,
    rng_state: u32,
}

impl ElasticBuffer {
    fn new(sample_rate: f32) -> Self {
        let length = (sample_rate * 2.75).ceil() as usize + 4;
        let initial_delay = sample_rate * 0.18;
        Self {
            left: vec![0.0; length],
            right: vec![0.0; length],
            write_index: 0,
            read_position: length as f32 - initial_delay,
            smooth_delay: initial_delay,
            jitter: 0.0,
            rng_state: 0xA341_316C,
        }
    }

    fn process(&mut self, left_in: f32, right_in: f32, control: ElasticControl) -> (f32, f32) {
        let len = self.left.len() as f32;

        self.left[self.write_index] = left_in;
        self.right[self.write_index] = right_in;

        let jitter_depth = 4.0 + control.grain_amount.powi(2) * 110.0;
        self.jitter = (self.jitter + next_signed(&mut self.rng_state) * 0.02).clamp(-1.0, 1.0);
        let jitter = if control.dirty {
            self.jitter + next_signed(&mut self.rng_state) * 0.25
        } else {
            self.jitter
        };

        let target_delay = (control.delay_samples + jitter * jitter_depth).max(8.0);
        let delay_smooth = 0.0018 + control.elasticity * 0.01;
        self.smooth_delay += (target_delay - self.smooth_delay) * delay_smooth;

        let desired_read = wrap_position(self.write_index as f32 - self.smooth_delay, len);
        let error = wrap_delta(desired_read - self.read_position, len);

        let mut speed = 1.0 + error * 0.003 + control.velocity * control.pitch_coupling * 0.48;
        if control.dirty {
            speed += next_signed(&mut self.rng_state) * 0.03 * control.grain_amount;
        }
        speed = speed.clamp(0.35, 1.65);

        self.read_position = wrap_position(self.read_position + speed, len);

        let out_l = read_cubic(&self.left, self.read_position);
        let out_r = read_cubic(&self.right, self.read_position);

        self.write_index = (self.write_index + 1) % self.left.len();
        (out_l, out_r)
    }
}

#[derive(Copy, Clone)]
struct WarpControl {
    tension: f32,
    diffusion: f32,
    elasticity: f32,
    air_damping: f32,
    air_compensation: bool,
    drift_phase_inc: f32,
    warp_motion: f32,
    color: WarpColor,
    character: CharacterMode,
}

struct SpectralWarp {
    low_state: f32,
    allpass_a: AllpassDelay,
    allpass_b: AllpassDelay,
    drift_phase: f32,
}

impl SpectralWarp {
    fn new(a_size: usize, b_size: usize) -> Self {
        Self {
            low_state: 0.0,
            allpass_a: AllpassDelay::new(a_size),
            allpass_b: AllpassDelay::new(b_size),
            drift_phase: 0.0,
        }
    }

    fn process(&mut self, input: f32, control: WarpControl) -> f32 {
        let color_damping_bias = match control.color {
            WarpColor::Neutral => 0.0,
            WarpColor::DarkDrag => 0.18,
            WarpColor::BrightShear => -0.15,
        };
        let damping = (control.air_damping * (0.3 + control.tension * 0.7) + color_damping_bias)
            .clamp(0.0, 0.98);
        let low_coeff = 0.012 + (1.0 - damping) * 0.12;
        self.low_state += (input - self.low_state) * low_coeff;

        let high = input - self.low_state;
        let compensation = if control.air_compensation {
            let color_boost = match control.color {
                WarpColor::Neutral => 1.0,
                WarpColor::DarkDrag => 0.75,
                WarpColor::BrightShear => 1.2,
            };
            damping * 0.72 * color_boost
        } else {
            0.0
        };
        let tone = self.low_state + high * (1.0 - damping * 0.9 + compensation);

        let g1 = (0.12
            + control.diffusion * (0.45 + control.elasticity * 0.22 + control.warp_motion * 0.24))
            .clamp(0.05, 0.9);
        let g2 = (0.1
            + control.diffusion * (0.38 + control.tension * 0.3 + control.warp_motion * 0.2))
            .clamp(0.05, 0.9);

        let mut output = self.allpass_a.process(tone, g1);
        output = self.allpass_b.process(output, g2);

        self.drift_phase = (self.drift_phase + control.drift_phase_inc).fract();
        let character_scale = match control.character {
            CharacterMode::Clean => 0.35,
            CharacterMode::Dirty => 1.0,
            CharacterMode::Crush => 1.2,
        };
        let drift = (self.drift_phase * TAU).sin()
            * (0.004 + control.tension * 0.02 + control.warp_motion * 0.018)
            * character_scale;

        output + high * drift
    }
}

#[derive(Default)]
struct SpaceStage {
    side_delay_a: ShortDelay,
    side_delay_b: ShortDelay,
    diff_left: AllpassDelay,
    diff_right: AllpassDelay,
}

impl SpaceStage {
    fn process(
        &mut self,
        left: f32,
        right: f32,
        width: f32,
        diffusion: f32,
        dirty: bool,
    ) -> (f32, f32) {
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        let delayed_a = self.side_delay_a.process(side);
        let delayed_b = self.side_delay_b.process(-side);
        let decorrelated = lerp(side, (delayed_a - delayed_b) * 0.5, width * 0.82);

        let spread = 1.0 + width * 0.78;
        let mut out_l = mid + decorrelated * spread;
        let mut out_r = mid - decorrelated * spread;

        let diffusion_gain = (0.14 + diffusion * 0.56).clamp(0.08, 0.8);
        let diffused_l = self.diff_left.process(out_l, diffusion_gain);
        let diffused_r = self.diff_right.process(out_r, diffusion_gain * 0.95);

        let blend = 0.1 + diffusion * 0.5;
        out_l = lerp(out_l, diffused_l, blend);
        out_r = lerp(out_r, diffused_r, blend);

        if dirty {
            out_l *= 1.015;
            out_r *= 1.015;
        }

        (out_l, out_r)
    }
}

#[derive(Default)]
struct PreEmphasis {
    low_state: f32,
}

impl PreEmphasis {
    fn process(&mut self, input: f32, tension: f32, continuity: f32) -> f32 {
        let coeff = (0.016 + (1.0 - tension) * 0.03).clamp(0.006, 0.08);
        self.low_state += (input - self.low_state) * coeff;
        let high = input - self.low_state;
        let boost = 0.06 + (1.0 - continuity) * 0.2;
        input + high * boost
    }
}

struct AllpassDelay {
    buffer: Vec<f32>,
    index: usize,
}

impl Default for AllpassDelay {
    fn default() -> Self {
        Self::new(31)
    }
}

impl AllpassDelay {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(2)],
            index: 0,
        }
    }

    fn process(&mut self, input: f32, gain: f32) -> f32 {
        let delayed = self.buffer[self.index];
        let output = -gain * input + delayed;
        self.buffer[self.index] = input + gain * output;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct ShortDelay {
    buffer: Vec<f32>,
    index: usize,
}

impl Default for ShortDelay {
    fn default() -> Self {
        Self::new(23)
    }
}

impl ShortDelay {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(2)],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.index];
        self.buffer[self.index] = input;
        self.index = (self.index + 1) % self.buffer.len();
        delayed
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

fn wrap_position(position: f32, length: f32) -> f32 {
    let mut wrapped = position;
    while wrapped < 0.0 {
        wrapped += length;
    }
    while wrapped >= length {
        wrapped -= length;
    }
    wrapped
}

fn wrap_delta(delta: f32, length: f32) -> f32 {
    let mut wrapped = delta;
    while wrapped > length * 0.5 {
        wrapped -= length;
    }
    while wrapped < -length * 0.5 {
        wrapped += length;
    }
    wrapped
}

fn read_cubic(buffer: &[f32], position: f32) -> f32 {
    let len = buffer.len() as isize;
    let base = position.floor() as isize;
    let frac = position - base as f32;

    let x0 = buffer[((base - 1).rem_euclid(len)) as usize];
    let x1 = buffer[(base.rem_euclid(len)) as usize];
    let x2 = buffer[((base + 1).rem_euclid(len)) as usize];
    let x3 = buffer[((base + 2).rem_euclid(len)) as usize];

    let a = (-0.5 * x0) + (1.5 * x1) - (1.5 * x2) + (0.5 * x3);
    let b = x0 - (2.5 * x1) + (2.0 * x2) - (0.5 * x3);
    let c = (-0.5 * x0) + (0.5 * x2);
    let d = x1;

    ((a * frac + b) * frac + c) * frac + d
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

fn crush(sample: f32) -> f32 {
    (sample * 128.0).round() / 128.0
}

fn soft_clip(input: f32) -> f32 {
    input / (1.0 + input.abs() * 0.6)
}

fn meter_norm(value: f32) -> f32 {
    (value / (1.0 + value)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{TensionFieldEngine, wrap_delta};
    use crate::clock::TransportState;
    use crate::params::TensionFieldParams;

    #[test]
    fn wrap_delta_picks_short_path() {
        let len = 100.0;
        assert!((wrap_delta(70.0, len) + 30.0).abs() < 1e-6);
        assert!((wrap_delta(-70.0, len) - 30.0).abs() < 1e-6);
    }

    #[test]
    fn render_stays_finite_under_extreme_feedback() {
        let params = TensionFieldParams::new();
        params.set_param(crate::params::PARAM_FEEDBACK_ID, 0.7);
        params.set_param(crate::params::PARAM_DUCKING_ID, 0.0);
        let settings = params.settings();

        let mut engine = TensionFieldEngine::new(48_000.0);
        let mut left = vec![0.0_f32; 2048];
        let mut right = vec![0.0_f32; 2048];
        left[0] = 1.0;
        right[0] = 1.0;

        for _ in 0..64 {
            let _ = engine.render(
                &settings,
                &mut left,
                &mut right,
                TransportState {
                    tempo_bpm: 120.0,
                    is_playing: true,
                    song_pos_beats: None,
                },
            );
            assert!(left.iter().all(|sample| sample.is_finite()));
            assert!(right.iter().all(|sample| sample.is_finite()));
        }
    }
}
