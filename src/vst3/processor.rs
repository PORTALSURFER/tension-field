//! VST3 processor using the shared Tension Field engine.

use std::cell::UnsafeCell;
use std::mem::size_of;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::slice;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use toybox::vst3::prelude::Steinberg::*;
use toybox::vst3::prelude::*;

use crate::dsp::RenderReport;

use super::param_bridge;
use super::shared_state::{
    PARAM_EVENT_CAPACITY, ParamEvent, TensionFieldVst3Runtime, TensionFieldVst3Shared,
};
use super::transport_utils::transport_state_from_process_context;
use super::{CONTROLLER_CID, read_vst3_state, write_vst3_state};

/// Exclusive audio-side borrow of Toybox's VST3 runtime owner.
pub(super) struct RealtimeRuntime {
    inner: UnsafeCell<AudioRuntime<TensionFieldVst3Runtime>>,
    in_process: AtomicBool,
}

// SAFETY: the atomic flag grants at most one mutable runtime borrow. Lifecycle
// callbacks publish replacements and never access the AudioRuntime directly.
unsafe impl Sync for RealtimeRuntime {}
unsafe impl Send for RealtimeRuntime {}

impl RealtimeRuntime {
    fn new(runtime: AudioRuntime<TensionFieldVst3Runtime>) -> Self {
        Self {
            inner: UnsafeCell::new(runtime),
            in_process: AtomicBool::new(false),
        }
    }

    fn try_acquire(&self) -> Option<RealtimeRuntimeGuard<'_>> {
        self.in_process
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| RealtimeRuntimeGuard { owner: self })
    }
}

struct RealtimeRuntimeGuard<'a> {
    owner: &'a RealtimeRuntime,
}

impl Deref for RealtimeRuntimeGuard<'_> {
    type Target = AudioRuntime<TensionFieldVst3Runtime>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: successful acquisition gives this guard exclusive access.
        unsafe { &*self.owner.inner.get() }
    }
}

impl DerefMut for RealtimeRuntimeGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: successful acquisition gives this guard exclusive access.
        unsafe { &mut *self.owner.inner.get() }
    }
}

impl Drop for RealtimeRuntimeGuard<'_> {
    fn drop(&mut self) {
        self.owner.in_process.store(false, Ordering::Release);
    }
}

/// VST3 processor component for Tension Field.
pub(super) struct TensionFieldVst3Processor {
    connection: InstanceConnection<TensionFieldVst3Shared>,
    /// Processor-owned canonical state; process never resolves it through the connection lock.
    shared: Arc<TensionFieldVst3Shared>,
    runtime: RealtimeRuntime,
    publisher: RuntimePublisher<TensionFieldVst3Runtime>,
    sample_rate_bits: AtomicU32,
}

impl TensionFieldVst3Processor {
    /// Create a processor with a fully initialized 48 kHz runtime.
    pub(super) fn new() -> Self {
        let shared = TensionFieldVst3Shared::new();
        let (publisher, audio_runtime) =
            RuntimePublisher::new(TensionFieldVst3Runtime::new(&shared.params, 48_000.0));
        Self {
            connection: InstanceConnection::new(
                InstanceConnectionRole::Processor,
                Arc::clone(&shared),
            ),
            shared,
            runtime: RealtimeRuntime::new(audio_runtime),
            publisher,
            sample_rate_bits: AtomicU32::new(48_000.0f32.to_bits()),
        }
    }

    fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate_bits.load(Ordering::Acquire)).max(1.0)
    }

    fn publish_runtime(&self, sample_rate: f32) {
        let Ok(registration) = self.publisher.register() else {
            return;
        };
        registration.publish(TensionFieldVst3Runtime::new(
            &self.shared.params,
            sample_rate,
        ));
        let _ = self.publisher.reclaim();
    }
}

impl Drop for TensionFieldVst3Processor {
    fn drop(&mut self) {
        let _ = self.publisher.reclaim();
    }
}

impl Class for TensionFieldVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IConnectionPoint,
        IToyboxSharedState,
    );
}

toybox::impl_vst3_instance_connection!(TensionFieldVst3Processor, connection);

impl IPluginBaseTrait for TensionFieldVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for TensionFieldVst3Processor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        unsafe { *class_id = CONTROLLER_CID };
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, direction: BusDirection) -> int32 {
        match media_type as MediaTypes {
            MediaTypes_::kAudio => match direction as BusDirections {
                BusDirections_::kInput | BusDirections_::kOutput => 1,
                _ => 0,
            },
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        direction: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult {
        if bus.is_null() || index != 0 || media_type as MediaTypes != MediaTypes_::kAudio {
            return kInvalidArgument;
        }
        let label = match direction as BusDirections {
            BusDirections_::kInput => "Input",
            BusDirections_::kOutput => "Output",
            _ => return kInvalidArgument,
        };
        let bus = unsafe { &mut *bus };
        bus.mediaType = MediaTypes_::kAudio as MediaType;
        bus.direction = direction;
        bus.channelCount = 2;
        copy_wstring(label, &mut bus.name);
        bus.busType = BusTypes_::kMain as BusType;
        bus.flags = BusInfo_::BusFlags_::kDefaultActive;
        kResultOk
    }

    unsafe fn getRoutingInfo(
        &self,
        _input: *mut RoutingInfo,
        _output: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        media_type: MediaType,
        direction: BusDirection,
        index: int32,
        _state: TBool,
    ) -> tresult {
        if media_type as MediaTypes != MediaTypes_::kAudio || index != 0 {
            return kInvalidArgument;
        }
        match direction as BusDirections {
            BusDirections_::kInput | BusDirections_::kOutput => kResultOk,
            _ => kInvalidArgument,
        }
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        self.publish_runtime(self.sample_rate());
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        unsafe { read_vst3_state(state, &self.shared) }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        unsafe { write_vst3_state(state, &self.shared) }
    }
}

impl IAudioProcessorTrait for TensionFieldVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        input_count: int32,
        outputs: *mut SpeakerArrangement,
        output_count: int32,
    ) -> tresult {
        if input_count != 1 || output_count != 1 || inputs.is_null() || outputs.is_null() {
            return kResultFalse;
        }
        if unsafe { *inputs } != SpeakerArr::kStereo || unsafe { *outputs } != SpeakerArr::kStereo {
            return kResultFalse;
        }
        kResultTrue
    }

    unsafe fn getBusArrangement(
        &self,
        direction: BusDirection,
        index: int32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        if arrangement.is_null() || index != 0 {
            return kInvalidArgument;
        }
        match direction as BusDirections {
            BusDirections_::kInput | BusDirections_::kOutput => {
                unsafe { *arrangement = SpeakerArr::kStereo };
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn canProcessSampleSize(&self, sample_size: int32) -> tresult {
        match sample_size as SymbolicSampleSizes {
            SymbolicSampleSizes_::kSample32 => kResultOk,
            SymbolicSampleSizes_::kSample64 => kNotImplemented,
            _ => kInvalidArgument,
        }
    }

    unsafe fn getLatencySamples(&self) -> uint32 {
        0
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }
        let sample_rate = unsafe { (*setup).sampleRate };
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return kInvalidArgument;
        }
        let sample_rate = sample_rate as f32;
        self.sample_rate_bits
            .store(sample_rate.to_bits(), Ordering::Release);
        self.publish_runtime(sample_rate);
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        self.publish_runtime(self.sample_rate());
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let data = unsafe { &*data };
        if data.numSamples < 0
            || (data.numSamples > 0
                && data.symbolicSampleSize != SymbolicSampleSizes_::kSample32 as int32)
        {
            return kInvalidArgument;
        }
        let Some(mut guard) = self.runtime.try_acquire() else {
            return kResultFalse;
        };
        let _ = guard.try_adopt(|_, _| true);
        let runtime = guard.current_mut();
        let frames = data.numSamples as usize;
        runtime.timeline.begin_block(frames);
        // SAFETY: the host owns the parameter-change collection for this
        // synchronous callback. The collector caps queue and point reads.
        unsafe { collect_parameter_events(data.inputParameterChanges, &mut runtime.timeline) };
        runtime.timeline.prepare();

        if frames == 0 {
            apply_remaining_events(runtime, &self.shared.params);
            return kResultOk;
        }

        if data.symbolicSampleSize != SymbolicSampleSizes_::kSample32 as int32 {
            apply_remaining_events(runtime, &self.shared.params);
            return kInvalidArgument;
        }
        // SAFETY: raw_stereo_buffers validates all host-owned pointers and
        // channel counts before any pointer arithmetic occurs.
        let Some(buffers) = (unsafe { raw_stereo_buffers(data) }) else {
            apply_remaining_events(runtime, &self.shared.params);
            return kInvalidArgument;
        };

        let transport = transport_state_from_process_context(data.processContext);
        let report = render_scheduled(runtime, &self.shared.params, buffers, transport);
        self.shared.status.update(report);
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        u32::MAX
    }
}

impl IProcessContextRequirementsTrait for TensionFieldVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> uint32 {
        IProcessContextRequirements_::Flags_::kNeedTempo
            | IProcessContextRequirements_::Flags_::kNeedProjectTimeMusic
            | IProcessContextRequirements_::Flags_::kNeedTransportState
    }
}

/// Bound the host-side parameter queue surface before entering the event loop.
const MAX_PARAMETER_QUEUES: usize = crate::params::STATE_VALUE_COUNT;

/// Read a bounded prefix plus one final point from each host parameter queue.
///
/// The prefix preserves sample-accurate points while the final point keeps the
/// parameter convergent when a host supplies more points than the fixed event
/// timeline can retain. Queue count and point reads are both bounded.
unsafe fn collect_parameter_events(
    changes: *mut IParameterChanges,
    timeline: &mut super::shared_state::ParamTimeline,
) {
    let Some(changes) = (unsafe { ComRef::from_raw(changes) }) else {
        return;
    };
    let queue_count = unsafe { changes.getParameterCount() }
        .max(0)
        .try_into()
        .unwrap_or(usize::MAX)
        .min(MAX_PARAMETER_QUEUES);
    let mut remaining = PARAM_EVENT_CAPACITY;

    for queue_index in 0..queue_count {
        let Some(queue) =
            (unsafe { ComRef::from_raw(changes.getParameterData(queue_index as int32)) })
        else {
            continue;
        };
        let param_id = unsafe { queue.getParameterId() };
        let point_count = unsafe { queue.getPointCount() }
            .max(0)
            .try_into()
            .unwrap_or(usize::MAX);
        let prefix_count = point_count.min(remaining);

        for point_index in 0..prefix_count {
            let mut offset = 0;
            let mut normalized = 0.0;
            if unsafe { queue.getPoint(point_index as int32, &mut offset, &mut normalized) }
                == kResultTrue
            {
                push_normalized_param_event(timeline, param_id, offset, normalized);
            }
            remaining = remaining.saturating_sub(1);
        }

        if prefix_count < point_count {
            let mut offset = 0;
            let mut normalized = 0.0;
            if unsafe { queue.getPoint((point_count - 1) as int32, &mut offset, &mut normalized) }
                == kResultTrue
            {
                push_normalized_param_event(timeline, param_id, offset, normalized);
            }
        }
    }
}

fn push_normalized_param_event(
    timeline: &mut super::shared_state::ParamTimeline,
    param_id: ParamID,
    offset: int32,
    normalized: ParamValue,
) {
    let Some(clap_id) = param_bridge::clap_id(param_id) else {
        return;
    };
    let Some(value) = param_bridge::from_normalized(param_id, normalized) else {
        return;
    };
    timeline.push(
        offset,
        ParamEvent {
            offset: 0,
            param_id: clap_id,
            value: value as f32,
        },
    );
}

#[derive(Clone, Copy)]
struct RawStereoBuffers {
    frames: usize,
    input_left: *const f32,
    input_right: *const f32,
    output_left: *mut f32,
    output_right: *mut f32,
}

fn address_range(pointer: *const f32, frames: usize) -> Option<(usize, usize)> {
    let bytes = frames.checked_mul(size_of::<f32>())?;
    let start = pointer as usize;
    Some((start, start.checked_add(bytes)?))
}

fn ranges_overlap(first: (usize, usize), second: (usize, usize)) -> bool {
    first.0 < second.1 && second.0 < first.1
}

fn validate_stereo_aliases(
    input_left: *const f32,
    input_right: *const f32,
    output_left: *mut f32,
    output_right: *mut f32,
    frames: usize,
) -> bool {
    let Some(input_left_range) = address_range(input_left, frames) else {
        return false;
    };
    let Some(input_right_range) = address_range(input_right, frames) else {
        return false;
    };
    let Some(output_left_range) = address_range(output_left, frames) else {
        return false;
    };
    let Some(output_right_range) = address_range(output_right, frames) else {
        return false;
    };

    if ranges_overlap(input_left_range, input_right_range)
        || ranges_overlap(output_left_range, output_right_range)
        || ranges_overlap(output_left_range, input_right_range)
        || ranges_overlap(output_right_range, input_left_range)
    {
        return false;
    }

    let output_left_overlaps_input = ranges_overlap(output_left_range, input_left_range);
    let output_right_overlaps_input = ranges_overlap(output_right_range, input_right_range);
    (!output_left_overlaps_input || ptr::eq(output_left as *const f32, input_left))
        && (!output_right_overlaps_input || ptr::eq(output_right as *const f32, input_right))
}

unsafe fn raw_stereo_buffers(data: &ProcessData) -> Option<RawStereoBuffers> {
    if data.numInputs != 1
        || data.numOutputs != 1
        || data.inputs.is_null()
        || data.outputs.is_null()
    {
        return None;
    }
    let input = unsafe { &*data.inputs };
    let output = unsafe { &*data.outputs };
    let input_buffers = unsafe { input.__field0.channelBuffers32 };
    let output_buffers = unsafe { output.__field0.channelBuffers32 };
    if input.numChannels != 2
        || output.numChannels != 2
        || input_buffers.is_null()
        || output_buffers.is_null()
    {
        return None;
    }
    let input_channels = unsafe { slice::from_raw_parts(input_buffers, 2) };
    let output_channels = unsafe { slice::from_raw_parts(output_buffers, 2) };
    if input_channels.iter().any(|channel| channel.is_null())
        || output_channels.iter().any(|channel| channel.is_null())
    {
        return None;
    }
    if !validate_stereo_aliases(
        input_channels[0],
        input_channels[1],
        output_channels[0],
        output_channels[1],
        usize::try_from(data.numSamples).ok()?,
    ) {
        return None;
    }
    Some(RawStereoBuffers {
        frames: usize::try_from(data.numSamples).ok()?,
        input_left: input_channels[0],
        input_right: input_channels[1],
        output_left: output_channels[0],
        output_right: output_channels[1],
    })
}

fn apply_remaining_events(
    runtime: &mut TensionFieldVst3Runtime,
    params: &crate::params::TensionFieldParams,
) {
    for index in 0..runtime.timeline.len() {
        if let Some(event) = runtime.timeline.get(index) {
            params.set_param(event.param_id, event.value);
        }
    }
    runtime.timeline.apply_overflow(params);
}

fn render_scheduled(
    runtime: &mut TensionFieldVst3Runtime,
    params: &crate::params::TensionFieldParams,
    buffers: RawStereoBuffers,
    transport: crate::clock::TransportState,
) -> RenderReport {
    let mut report = RenderReport::default();
    let mut event_index = 0usize;
    let mut cursor = 0usize;
    let mut sent_transport = false;

    while cursor < buffers.frames {
        while let Some(event) = runtime.timeline.get(event_index) {
            if event.offset > cursor {
                break;
            }
            params.set_param(event.param_id, event.value);
            event_index += 1;
        }

        let next = runtime
            .timeline
            .get(event_index)
            .map(|event| event.offset)
            .filter(|offset| *offset > cursor)
            .unwrap_or(buffers.frames)
            .min(buffers.frames);
        if next <= cursor {
            continue;
        }
        let length = next - cursor;
        // SAFETY: all pointers were validated for the full host block and the
        // segment is bounded by that block's frame count.
        unsafe {
            ptr::copy(
                buffers.input_left.add(cursor),
                buffers.output_left.add(cursor),
                length,
            );
            ptr::copy(
                buffers.input_right.add(cursor),
                buffers.output_right.add(cursor),
                length,
            );
            let left = slice::from_raw_parts_mut(buffers.output_left.add(cursor), length);
            let right = slice::from_raw_parts_mut(buffers.output_right.add(cursor), length);
            let segment_transport = if sent_transport {
                crate::clock::TransportState {
                    song_pos_beats: None,
                    ..transport
                }
            } else {
                transport
            };
            report.merge_max(runtime.engine.render(
                &params.settings(),
                left,
                right,
                segment_transport,
            ));
        }
        sent_transport = true;
        cursor = next;
    }

    while let Some(event) = runtime.timeline.get(event_index) {
        params.set_param(event.param_id, event.value);
        event_index += 1;
    }
    runtime.timeline.apply_overflow(params);
    report
}

#[cfg(test)]
mod tests {
    use super::validate_stereo_aliases;

    #[test]
    fn stereo_alias_validation_accepts_in_place_and_rejects_cross_aliases() {
        let mut input_left = [0.0_f32; 8];
        let mut input_right = [0.0_f32; 8];
        let mut output_left = [0.0_f32; 8];
        let mut output_right = [0.0_f32; 8];

        assert!(validate_stereo_aliases(
            input_left.as_ptr(),
            input_right.as_ptr(),
            input_left.as_mut_ptr(),
            input_right.as_mut_ptr(),
            input_left.len(),
        ));
        assert!(validate_stereo_aliases(
            input_left.as_ptr(),
            input_right.as_ptr(),
            output_left.as_mut_ptr(),
            output_right.as_mut_ptr(),
            input_left.len(),
        ));
        assert!(!validate_stereo_aliases(
            input_left.as_ptr(),
            input_right.as_ptr(),
            input_right.as_mut_ptr(),
            input_left.as_mut_ptr(),
            input_left.len(),
        ));
        assert!(!validate_stereo_aliases(
            input_left.as_ptr(),
            input_left.as_ptr(),
            output_left.as_mut_ptr(),
            output_right.as_mut_ptr(),
            input_left.len(),
        ));
    }
}
