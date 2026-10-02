//! Shared VST3 state and bounded parameter-event storage.

use std::sync::{Arc, Mutex};

use toybox::clack_plugin::utils::ClapId;
use toybox::vst3::prelude::ComPtr;
use toybox::vst3::prelude::Steinberg::Vst::IComponentHandler;

use crate::GuiStatus;
use crate::dsp::TensionFieldEngine;
use crate::params::{STATE_VALUE_COUNT, TensionFieldParams};

/// Maximum number of VST3 parameter points retained for one process block.
pub(super) const PARAM_EVENT_CAPACITY: usize = 16_384;

/// One normalized-to-plain parameter point scheduled by a VST3 host.
#[derive(Clone, Copy, Debug)]
pub(super) struct ParamEvent {
    /// Sample offset relative to the current process block.
    pub(super) offset: usize,
    /// Shared CLAP/VST3 parameter identity.
    pub(super) param_id: ClapId,
    /// Canonical plain parameter value.
    pub(super) value: f32,
}

/// Fixed-capacity, allocation-free process-block parameter timeline.
pub(super) struct ParamTimeline {
    events: Vec<ParamEvent>,
    overflow_final: [Option<ParamEvent>; STATE_VALUE_COUNT],
    block_frames: usize,
}

impl ParamTimeline {
    /// Create a timeline with all process-time storage reserved up front.
    pub(super) fn new() -> Self {
        Self {
            events: Vec::with_capacity(PARAM_EVENT_CAPACITY),
            overflow_final: [None; STATE_VALUE_COUNT],
            block_frames: 0,
        }
    }

    /// Start collecting one process block.
    pub(super) fn begin_block(&mut self, block_frames: usize) {
        self.events.clear();
        self.overflow_final = [None; STATE_VALUE_COUNT];
        self.block_frames = block_frames;
    }

    /// Add one point without growing the process-time vector.
    pub(super) fn push(&mut self, offset: i32, event: ParamEvent) {
        let Some(index) = event
            .param_id
            .get()
            .checked_sub(1)
            .and_then(|id| usize::try_from(id).ok())
            .filter(|index| *index < STATE_VALUE_COUNT)
        else {
            return;
        };
        let mut event = event;
        event.offset = usize::try_from(offset.max(0))
            .unwrap_or(self.block_frames)
            .min(self.block_frames);
        if self.events.len() < PARAM_EVENT_CAPACITY {
            self.events.push(event);
        } else {
            self.overflow_final[index] = Some(event);
        }
    }

    /// Sort points into chronological order with stable tie order not required.
    pub(super) fn prepare(&mut self) {
        self.events.sort_unstable_by_key(|event| event.offset);
    }

    /// Return one scheduled point by index.
    pub(super) fn get(&self, index: usize) -> Option<ParamEvent> {
        self.events.get(index).copied()
    }

    /// Return the number of retained points.
    pub(super) fn len(&self) -> usize {
        self.events.len()
    }

    /// Apply all overflow points after the block so the next block converges.
    pub(super) fn apply_overflow(&self, params: &TensionFieldParams) {
        for event in self.overflow_final.iter().flatten() {
            params.set_param(event.param_id, event.value);
        }
    }
}

/// State shared by one VST3 processor/controller pair and its hosted editor.
pub(super) struct TensionFieldVst3Shared {
    /// Atomic parameter storage shared with the DSP engine.
    pub(super) params: Arc<TensionFieldParams>,
    /// Realtime meter storage consumed by the Radiant editor.
    pub(super) status: Arc<GuiStatus>,
    /// Host component handler used by VST3 UI-originated automation.
    pub(super) component_handler: Mutex<Option<ComPtr<IComponentHandler>>>,
}

impl TensionFieldVst3Shared {
    /// Create one unconnected VST3 endpoint's initial shared state.
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            params: Arc::new(TensionFieldParams::new()),
            status: Arc::new(GuiStatus::default()),
            component_handler: Mutex::new(None),
        })
    }
}

/// Audio-owned DSP runtime published through Toybox's bounded handoff API.
pub(super) struct TensionFieldVst3Runtime {
    /// Format-neutral Tension Field DSP.
    pub(super) engine: TensionFieldEngine,
    /// Per-block VST3 automation storage.
    pub(super) timeline: ParamTimeline,
}

impl TensionFieldVst3Runtime {
    /// Build a complete runtime on a non-audio thread.
    pub(super) fn new(params: &TensionFieldParams, sample_rate: f32) -> Self {
        let _ = params;
        Self {
            engine: TensionFieldEngine::new(sample_rate),
            timeline: ParamTimeline::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(param_id: u32, value: f32) -> ParamEvent {
        ParamEvent {
            offset: 0,
            param_id: ClapId::new(param_id),
            value,
        }
    }

    #[test]
    fn timeline_clamps_offsets_and_sorts_points() {
        let mut timeline = ParamTimeline::new();
        timeline.begin_block(64);
        timeline.push(64, event(1, 1.0));
        timeline.push(-1, event(2, 2.0));
        timeline.push(12, event(3, 3.0));
        timeline.prepare();

        assert_eq!(timeline.get(0).map(|item| item.offset), Some(0));
        assert_eq!(timeline.get(1).map(|item| item.offset), Some(12));
        assert_eq!(timeline.get(2).map(|item| item.offset), Some(64));
    }

    #[test]
    fn timeline_overflow_retains_the_last_value_per_parameter() {
        let mut timeline = ParamTimeline::new();
        timeline.begin_block(8);
        for value in 0..=PARAM_EVENT_CAPACITY {
            let value = if value == PARAM_EVENT_CAPACITY {
                0.75
            } else {
                0.0
            };
            timeline.push(2, event(1, value));
        }

        assert_eq!(timeline.len(), PARAM_EVENT_CAPACITY);
        let params = TensionFieldParams::new();
        timeline.apply_overflow(&params);
        assert_eq!(params.get_param(ClapId::new(1)), Some(0.75));
    }
}
