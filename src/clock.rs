//! Transport-aware timing helpers for synced gesture and modulation engines.

use crate::params::PullDivision;

/// Transport metadata needed by Tension Field's timing engines.
#[derive(Debug, Copy, Clone)]
pub(crate) struct TransportState {
    /// Host tempo in beats per minute.
    pub tempo_bpm: f32,
    /// Whether the host reports active playback.
    pub is_playing: bool,
    /// Song position in quarter-note beats when available.
    pub song_pos_beats: Option<f64>,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            tempo_bpm: 120.0,
            is_playing: false,
            song_pos_beats: None,
        }
    }
}

/// Per-sample clock snapshot shared by DSP subsystems.
#[derive(Debug, Copy, Clone)]
pub(crate) struct ClockFrame {
    /// Continuous beat position.
    pub beat_position: f64,
    /// Playback state flag.
    pub is_playing: bool,
}

impl ClockFrame {
    /// Return normalized phase within one cycle of `division`, including swing warp.
    pub(crate) fn phase_for_division(self, division: PullDivision, swing: f32) -> f32 {
        let beats = division.beats_per_cycle().max(1.0e-4) as f64;
        let raw = (self.beat_position / beats).fract() as f32;
        apply_swing(raw, swing)
    }
}

/// Running transport clock with fallback behavior when hosts omit timeline data.
pub(crate) struct TransportClock {
    sample_rate: f32,
    fallback_beat_position: f64,
}

impl TransportClock {
    /// Create a clock for the given sample rate.
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            fallback_beat_position: 0.0,
        }
    }

    /// Advance one sample and return the current transport frame.
    pub(crate) fn tick(&mut self, transport: TransportState) -> ClockFrame {
        let tempo_bpm = transport.tempo_bpm.clamp(20.0, 300.0);
        let beat_increment = tempo_bpm as f64 / (self.sample_rate as f64 * 60.0);

        let beat_position = transport
            .song_pos_beats
            .unwrap_or(self.fallback_beat_position);

        if transport.is_playing {
            self.fallback_beat_position = beat_position + beat_increment;
        } else {
            self.fallback_beat_position = beat_position;
        }

        ClockFrame {
            beat_position,
            is_playing: transport.is_playing,
        }
    }
}

/// Warp phase with a swing amount while preserving `[0, 1]` bounds.
pub(crate) fn apply_swing(phase: f32, swing: f32) -> f32 {
    let p = phase.fract();
    let split = (0.5 + swing.clamp(0.0, 1.0) * 0.24).clamp(0.1, 0.9);
    if p < split {
        (p / split) * 0.5
    } else {
        0.5 + ((p - split) / (1.0 - split)) * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::{TransportClock, TransportState, apply_swing};

    #[test]
    fn swing_warp_stays_in_unit_range() {
        for i in 0..64 {
            let phase = i as f32 / 64.0;
            let warped = apply_swing(phase, 1.0);
            assert!((0.0..=1.0).contains(&warped));
        }
    }

    #[test]
    fn clock_advances_when_playing_without_song_position() {
        let mut clock = TransportClock::new(48_000.0);
        let first = clock.tick(TransportState {
            tempo_bpm: 120.0,
            is_playing: true,
            song_pos_beats: None,
        });
        let second = clock.tick(TransportState {
            tempo_bpm: 120.0,
            is_playing: true,
            song_pos_beats: None,
        });

        assert!(second.beat_position > first.beat_position);
    }
}
