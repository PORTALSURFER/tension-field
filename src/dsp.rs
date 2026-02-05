//! Core DSP for the Tension Field effect.

use std::f32::consts::TAU;

use crate::params::{PullShape, TensionFieldSettings};

/// Audio engine implementing the elastic buffer, spectral warp, and space stage.
pub(crate) struct TensionFieldEngine {
    sample_rate: f32,
    pre_left: PreEmphasis,
    pre_right: PreEmphasis,
    gesture: GestureEngine,
    elastic: ElasticBuffer,
    warp_left: SpectralWarp,
    warp_right: SpectralWarp,
    space: SpaceStage,
    feedback_left: f32,
    feedback_right: f32,
}

impl TensionFieldEngine {
    /// Create a new Tension Field engine at the given sample rate.
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            pre_left: PreEmphasis::default(),
            pre_right: PreEmphasis::default(),
            gesture: GestureEngine::default(),
            elastic: ElasticBuffer::new(sample_rate),
            warp_left: SpectralWarp::new(37, 73),
            warp_right: SpectralWarp::new(43, 79),
            space: SpaceStage::default(),
            feedback_left: 0.0,
            feedback_right: 0.0,
        }
    }

    /// Process one stereo block in place.
    pub(crate) fn render(
        &mut self,
        settings: &TensionFieldSettings,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let frames = left.len().min(right.len());
        if frames == 0 {
            return;
        }

        for (l, r) in left.iter_mut().zip(right.iter_mut()).take(frames) {
            let gesture = self.gesture.next(settings, self.sample_rate);
            let feedback_l = self.feedback_left * settings.feedback;
            let feedback_r = self.feedback_right * settings.feedback;

            let pre_l = self.pre_left.process(
                *l + feedback_l,
                gesture.tension_drive,
                settings.grain_continuity,
            );
            let pre_r = self.pre_right.process(
                *r + feedback_r,
                gesture.tension_drive,
                settings.grain_continuity,
            );

            let (elastic_l, elastic_r) = self.elastic.process(
                pre_l,
                pre_r,
                ElasticControl {
                    delay_samples: gesture.delay_samples,
                    velocity: gesture.velocity,
                    pitch_coupling: settings.pitch_coupling,
                    grain_amount: settings.grain_continuity,
                    elasticity: settings.elasticity,
                    dirty: settings.dirty,
                },
            );

            let warp_control = WarpControl {
                tension: gesture.tension_drive,
                diffusion: settings.diffusion,
                elasticity: settings.elasticity,
                air_damping: settings.air_damping,
                air_compensation: settings.air_compensation,
                drift_phase_inc: gesture.drift_phase_inc,
                dirty: settings.dirty,
            };
            let warped_l = self.warp_left.process(elastic_l, warp_control);
            let warped_r = self.warp_right.process(elastic_r, warp_control);

            let (space_l, space_r) = self.space.process(
                warped_l,
                warped_r,
                settings.width,
                settings.diffusion,
                settings.dirty,
            );

            let out_l = soft_clip(space_l);
            let out_r = soft_clip(space_r);
            *l = out_l;
            *r = out_r;
            self.feedback_left = out_l;
            self.feedback_right = out_r;
        }
    }
}

#[derive(Copy, Clone)]
struct GestureFrame {
    delay_samples: f32,
    velocity: f32,
    tension_drive: f32,
    drift_phase_inc: f32,
}

#[derive(Default)]
struct GestureEngine {
    phase: f32,
    held_value: f32,
    held_directional: f32,
    pull_env: f32,
    random_walk: f32,
    was_hold: bool,
    previous_value: f32,
    rng_state: u32,
}

impl GestureEngine {
    fn next(&mut self, settings: &TensionFieldSettings, sample_rate: f32) -> GestureFrame {
        if self.rng_state == 0 {
            self.rng_state = 0x9E37_79B9;
        }

        let phase_increment = (settings.pull_rate_hz / sample_rate).clamp(0.000_01, 0.1);
        if !settings.hold {
            self.phase = (self.phase + phase_increment).fract();
        }

        let shape_value = evaluate_shape(settings.pull_shape, self.phase);
        let entering_hold = settings.hold && !self.was_hold;
        if entering_hold {
            self.held_value = shape_value;
        }
        let active_shape = if settings.hold {
            self.held_value
        } else {
            shape_value
        };

        if !settings.hold {
            let pull_target = if settings.pull_trigger { 1.0 } else { 0.0 };
            let attack = 0.004 + settings.elasticity * 0.03;
            let release = 0.0008 + settings.rebound * 0.02;
            let smoothing = if pull_target > self.pull_env {
                attack
            } else {
                release
            };
            self.pull_env += (pull_target - self.pull_env) * smoothing;

            let walk_amount = 0.0015 + settings.elasticity * 0.007;
            self.random_walk = (self.random_walk + next_signed(&mut self.rng_state) * walk_amount)
                .clamp(-1.0, 1.0);
        }

        let motion = active_shape * (0.35 + 0.65 * self.pull_env)
            + self.random_walk * (0.06 + settings.elasticity * 0.14);
        let mut directional = (motion * settings.pull_direction).clamp(-1.0, 1.0);
        if settings.hold {
            if entering_hold {
                self.held_directional = directional;
            }
            directional = self.held_directional;
        } else {
            self.held_directional = directional;
        }

        let velocity = if settings.hold {
            0.0
        } else {
            directional - self.previous_value
        };
        self.previous_value = directional;
        self.was_hold = settings.hold;

        let tension_drive = (settings.tension * (0.25 + directional.abs() * 0.75)).clamp(0.0, 1.0);
        let center_delay = sample_rate * (0.06 + settings.tension * 0.22);
        let delay_swing = sample_rate * (0.005 + settings.elasticity * 0.06);
        let delay_samples = (center_delay + directional * delay_swing).max(16.0);

        let drift_phase_inc =
            (0.0002 + velocity.abs() * 0.02 + settings.pull_rate_hz * 0.0004).clamp(0.0001, 0.06);

        GestureFrame {
            delay_samples,
            velocity,
            tension_drive,
            drift_phase_inc,
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
    dirty: bool,
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
        let damping = (control.air_damping * (0.3 + control.tension * 0.7)).clamp(0.0, 0.98);
        let low_coeff = 0.012 + (1.0 - damping) * 0.12;
        self.low_state += (input - self.low_state) * low_coeff;

        let high = input - self.low_state;
        let compensation = if control.air_compensation {
            damping * 0.72
        } else {
            0.0
        };
        let tone = self.low_state + high * (1.0 - damping * 0.9 + compensation);

        let g1 = (0.12 + control.diffusion * (0.45 + control.elasticity * 0.22)).clamp(0.05, 0.85);
        let g2 = (0.1 + control.diffusion * (0.38 + control.tension * 0.3)).clamp(0.05, 0.85);

        let mut output = self.allpass_a.process(tone, g1);
        output = self.allpass_b.process(output, g2);

        self.drift_phase = (self.drift_phase + control.drift_phase_inc).fract();
        let dirty_scale = if control.dirty { 1.0 } else { 0.35 };
        let drift = (self.drift_phase * TAU).sin() * (0.004 + control.tension * 0.02) * dirty_scale;

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

fn soft_clip(input: f32) -> f32 {
    input / (1.0 + input.abs() * 0.6)
}

#[cfg(test)]
mod tests {
    use super::{GestureEngine, evaluate_shape, wrap_delta};
    use crate::params::{PullShape, TensionFieldSettings};

    fn settings() -> TensionFieldSettings {
        TensionFieldSettings {
            tension: 0.6,
            pull_rate_hz: 0.2,
            pull_shape: PullShape::Rubber,
            hold: false,
            grain_continuity: 0.25,
            pitch_coupling: 0.2,
            width: 0.5,
            diffusion: 0.6,
            air_damping: 0.4,
            air_compensation: true,
            pull_direction: 0.8,
            elasticity: 0.7,
            pull_trigger: true,
            rebound: 0.6,
            dirty: false,
            feedback: 0.1,
        }
    }

    #[test]
    fn shape_values_stay_in_range() {
        for shape in [
            PullShape::Linear,
            PullShape::Rubber,
            PullShape::Ratchet,
            PullShape::Wave,
        ] {
            for i in 0..64 {
                let phase = i as f32 / 64.0;
                let value = evaluate_shape(shape, phase);
                assert!(value >= -1.01 && value <= 1.01);
            }
        }
    }

    #[test]
    fn wrap_delta_picks_short_path() {
        let len = 100.0;
        assert!((wrap_delta(70.0, len) + 30.0).abs() < 1e-6);
        assert!((wrap_delta(-70.0, len) - 30.0).abs() < 1e-6);
    }

    #[test]
    fn hold_freezes_gesture_value() {
        let mut engine = GestureEngine::default();
        let mut cfg = settings();

        let _ = engine.next(&cfg, 48_000.0);
        cfg.hold = true;
        let held_a = engine.next(&cfg, 48_000.0).delay_samples;
        let held_b = engine.next(&cfg, 48_000.0).delay_samples;

        assert!((held_a - held_b).abs() < 1e-4);
    }
}
