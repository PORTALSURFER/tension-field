//! VST3 process-context conversion for the shared transport clock.

use toybox::vst3::prelude::Steinberg::Vst::{ProcessContext, ProcessContext_};

use crate::clock::TransportState;

/// Convert one optional VST3 process context into Tension Field transport data.
pub(super) fn transport_state_from_process_context(
    process_context: *mut ProcessContext,
) -> TransportState {
    let Some(context) = (unsafe { process_context.as_ref() }) else {
        return TransportState::default();
    };

    let state = context.state;
    let tempo_valid = (state & ProcessContext_::StatesAndFlags_::kTempoValid) != 0;
    let project_time_valid =
        (state & ProcessContext_::StatesAndFlags_::kProjectTimeMusicValid) != 0;
    TransportState {
        tempo_bpm: if tempo_valid {
            context.tempo as f32
        } else {
            120.0
        },
        is_playing: (state & ProcessContext_::StatesAndFlags_::kPlaying) != 0,
        song_pos_beats: project_time_valid.then_some(context.projectTimeMusic),
    }
}
