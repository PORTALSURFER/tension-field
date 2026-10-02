//! Tension Field CLAP effect.
//!
//! Tension Field applies a dual time-warp treatment designed for slow, elastic
//! motion. The effect combines a variable-speed elastic buffer with a spectral
//! drag stage and a spatial diffusion stage for wide atmospheric textures.

#![deny(missing_docs, warnings)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use toybox::clack_common::plugin::features as plugin_features;
use toybox::clack_extensions::audio_ports::*;
#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
use toybox::clack_extensions::gui::{PluginGui, PluginGuiImpl};
use toybox::clack_extensions::params::*;
use toybox::clack_extensions::state::{PluginState, PluginStateImpl};
use toybox::clack_plugin::events::event_types::{TransportEvent, TransportFlags};
use toybox::clack_plugin::prelude::*;
use toybox::clack_plugin::stream::{InputStream, OutputStream};
use toybox::clap::automation::{AutomationDrainBuffer, AutomationQueue};
use toybox::clap::params::apply_param_events;

mod clock;
mod dsp;
mod gesture;
#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
mod gui;
mod mod_matrix;
mod params;
mod state;
#[cfg(all(target_os = "macos", feature = "vst3"))]
mod vst3;

pub(crate) use dsp::{RenderReport, TensionFieldEngine};
pub(crate) use params::{
    TensionFieldParams, apply_state_values, param_count, state_values, text_to_value,
    value_to_text, write_param_info,
};
pub(crate) use state::{PluginStateSnapshot, read_snapshot, write_snapshot};

/// CLAP plugin type for Tension Field.
pub struct TensionFieldPlugin;

impl Plugin for TensionFieldPlugin {
    type AudioProcessor<'a> = TensionFieldAudioProcessor<'a>;
    type Shared<'a> = TensionFieldShared;
    type MainThread<'a> = TensionFieldMainThread<'a>;

    fn declare_extensions(
        builder: &mut PluginExtensions<Self>,
        _shared: Option<&Self::Shared<'_>>,
    ) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginState>();
        #[cfg(all(target_os = "macos", feature = "radiant-gui"))]
        {
            builder.register::<PluginGui>();
        }
    }
}

impl DefaultPluginFactory for TensionFieldPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("com.portalsurfer.tensionfield", "Tension Field")
            .with_vendor("PORTALSURFER")
            .with_version("0.1.0")
            .with_description("Elastic stretch and spectral drag audio effect")
            .with_features([plugin_features::AUDIO_EFFECT, plugin_features::STEREO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
        Ok(TensionFieldShared {
            params: Arc::new(TensionFieldParams::new()),
            automation_queue: Arc::new(AutomationQueue::default()),
            status: Arc::new(GuiStatus::default()),
        })
    }

    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        #[cfg(all(target_os = "macos", feature = "radiant-gui"))]
        {
            let param_requester = host_param_requester(host.shared());
            Ok(TensionFieldMainThread {
                shared,
                gui: gui::new_gui(
                    Arc::clone(&shared.params),
                    Arc::clone(&shared.automation_queue),
                    Arc::clone(&shared.status),
                    param_requester,
                ),
                automation_drain: AutomationDrainBuffer::default(),
            })
        }
        #[cfg(not(all(target_os = "macos", feature = "radiant-gui")))]
        {
            let _ = host;
            Ok(TensionFieldMainThread {
                shared,
                automation_drain: AutomationDrainBuffer::default(),
            })
        }
    }
}

/// Real-time status snapshot consumed by the GUI thread.
#[derive(Default)]
pub struct GuiStatus {
    input_left: AtomicU32,
    input_right: AtomicU32,
    elastic_activity: AtomicU32,
    warp_activity: AtomicU32,
    space_activity: AtomicU32,
    feedback_activity: AtomicU32,
    output_left: AtomicU32,
    output_right: AtomicU32,
    tension_activity: AtomicU32,
}

impl GuiStatus {
    fn update(&self, report: RenderReport) {
        self.input_left
            .store(f32_to_bits(report.input_left), Ordering::Relaxed);
        self.input_right
            .store(f32_to_bits(report.input_right), Ordering::Relaxed);
        self.elastic_activity
            .store(f32_to_bits(report.elastic_activity), Ordering::Relaxed);
        self.warp_activity
            .store(f32_to_bits(report.warp_activity), Ordering::Relaxed);
        self.space_activity
            .store(f32_to_bits(report.space_activity), Ordering::Relaxed);
        self.feedback_activity
            .store(f32_to_bits(report.feedback_activity), Ordering::Relaxed);
        self.output_left
            .store(f32_to_bits(report.output_left), Ordering::Relaxed);
        self.output_right
            .store(f32_to_bits(report.output_right), Ordering::Relaxed);
        self.tension_activity
            .store(f32_to_bits(report.tension_activity), Ordering::Relaxed);
    }

    fn snapshot(&self) -> [f32; state::METER_COUNT] {
        [
            bits_to_f32(self.input_left.load(Ordering::Relaxed)),
            bits_to_f32(self.input_right.load(Ordering::Relaxed)),
            bits_to_f32(self.elastic_activity.load(Ordering::Relaxed)),
            bits_to_f32(self.warp_activity.load(Ordering::Relaxed)),
            bits_to_f32(self.space_activity.load(Ordering::Relaxed)),
            bits_to_f32(self.feedback_activity.load(Ordering::Relaxed)),
            bits_to_f32(self.output_left.load(Ordering::Relaxed)),
            bits_to_f32(self.output_right.load(Ordering::Relaxed)),
            bits_to_f32(self.tension_activity.load(Ordering::Relaxed)),
        ]
    }

    fn apply_snapshot(&self, snapshot: [f32; state::METER_COUNT]) {
        self.input_left
            .store(f32_to_bits(snapshot[0]), Ordering::Relaxed);
        self.input_right
            .store(f32_to_bits(snapshot[1]), Ordering::Relaxed);
        self.elastic_activity
            .store(f32_to_bits(snapshot[2]), Ordering::Relaxed);
        self.warp_activity
            .store(f32_to_bits(snapshot[3]), Ordering::Relaxed);
        self.space_activity
            .store(f32_to_bits(snapshot[4]), Ordering::Relaxed);
        self.feedback_activity
            .store(f32_to_bits(snapshot[5]), Ordering::Relaxed);
        self.output_left
            .store(f32_to_bits(snapshot[6]), Ordering::Relaxed);
        self.output_right
            .store(f32_to_bits(snapshot[7]), Ordering::Relaxed);
        self.tension_activity
            .store(f32_to_bits(snapshot[8]), Ordering::Relaxed);
    }
}

/// Shared state between threads.
pub struct TensionFieldShared {
    /// Parameter storage shared between main and audio threads.
    params: Arc<TensionFieldParams>,
    /// Pending GUI automation events waiting for host flush.
    automation_queue: Arc<AutomationQueue>,
    /// Metering/status values produced by the audio thread.
    status: Arc<GuiStatus>,
}

impl PluginShared<'_> for TensionFieldShared {}

/// Helper for requesting parameter flushes from the GUI thread.
#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
#[derive(Clone, Copy)]
pub(crate) struct HostParamRequester {
    host: HostSharedHandle<'static>,
    params: HostParams,
}

#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
impl HostParamRequester {
    /// Request a host parameter flush.
    pub fn request_flush(self) {
        self.params.request_flush(&self.host);
    }
}

#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
fn host_param_requester(host: HostSharedHandle<'_>) -> Option<HostParamRequester> {
    let params = host.get_extension::<HostParams>()?;
    let host =
        unsafe { std::mem::transmute::<HostSharedHandle<'_>, HostSharedHandle<'static>>(host) };
    Some(HostParamRequester { host, params })
}

/// Main-thread state for host interaction and GUI hosting.
pub struct TensionFieldMainThread<'a> {
    shared: &'a TensionFieldShared,
    #[cfg(all(target_os = "macos", feature = "radiant-gui"))]
    gui: toybox::radiant_gui::RadiantHostedGui,
    automation_drain: AutomationDrainBuffer,
}

impl<'a> PluginMainThread<'a, TensionFieldShared> for TensionFieldMainThread<'a> {}

impl PluginAudioPortsImpl for TensionFieldMainThread<'_> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1
    }

    fn get(&mut self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }

        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: b"main",
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: None,
        })
    }
}

impl PluginMainThreadParams for TensionFieldMainThread<'_> {
    fn count(&mut self) -> u32 {
        param_count()
    }

    fn get_info(&mut self, param_index: u32, writer: &mut ParamInfoWriter) {
        write_param_info(param_index, writer);
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        self.shared
            .params
            .get_param(param_id)
            .map(|value| value as f64)
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        value_to_text(param_id, value, writer)
    }

    fn text_to_value(&mut self, param_id: ClapId, text: &std::ffi::CStr) -> Option<f64> {
        text_to_value(param_id, text)
    }

    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        output_parameter_changes: &mut OutputEvents,
    ) {
        apply_param_events(input_parameter_changes, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });
        let _ = self
            .automation_drain
            .drain(&self.shared.automation_queue, output_parameter_changes);
    }
}

impl PluginStateImpl for TensionFieldMainThread<'_> {
    fn save(&mut self, output: &mut OutputStream) -> Result<(), PluginError> {
        let snapshot = PluginStateSnapshot {
            param_values: state_values(&self.shared.params),
            meter_values: self.shared.status.snapshot(),
        };
        write_snapshot(output, &snapshot)?;
        Ok(())
    }

    fn load(&mut self, input: &mut InputStream) -> Result<(), PluginError> {
        let snapshot =
            read_snapshot(input).map_err(|error| PluginError::Message(error.as_message()))?;
        apply_state_values(&self.shared.params, snapshot.param_values);
        self.shared.status.apply_snapshot(snapshot.meter_values);
        Ok(())
    }
}

#[cfg(all(target_os = "macos", feature = "radiant-gui"))]
impl PluginGuiImpl for TensionFieldMainThread<'_> {
    toybox::radiant_clap_gui_callbacks!(
        gui = gui,
        preferred_size = gui::preferred_window_size,
        show = |_main_thread| Ok(())
    );
}

/// Audio-thread processor for Tension Field.
pub struct TensionFieldAudioProcessor<'a> {
    shared: &'a TensionFieldShared,
    engine: TensionFieldEngine,
    automation_drain: AutomationDrainBuffer,
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
}

impl<'a> PluginAudioProcessor<'a, TensionFieldShared, TensionFieldMainThread<'a>>
    for TensionFieldAudioProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut TensionFieldMainThread<'a>,
        shared: &'a TensionFieldShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            shared,
            engine: TensionFieldEngine::new(audio_config.sample_rate as f32),
            automation_drain: AutomationDrainBuffer::default(),
            scratch_left: Vec::new(),
            scratch_right: Vec::new(),
        })
    }

    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        apply_param_events(events.input, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });

        let settings = self.shared.params.settings();
        let transport = transport_state_from_transport(process.transport.copied());
        for mut port_pair in &mut audio {
            let Some(mut channels) = port_pair.channels()?.into_f32() else {
                continue;
            };

            let mut channel_iter = channels.iter_mut();
            let Some(left_pair) = channel_iter.next() else {
                continue;
            };
            let Some(right_pair) = channel_iter.next() else {
                continue;
            };

            self.process_stereo_pair(left_pair, right_pair, &settings, transport);
        }

        let _ = self
            .automation_drain
            .drain(&self.shared.automation_queue, events.output);

        Ok(ProcessStatus::Continue)
    }
}

impl TensionFieldAudioProcessor<'_> {
    fn process_stereo_pair(
        &mut self,
        left: ChannelPair<'_, f32>,
        right: ChannelPair<'_, f32>,
        settings: &params::TensionFieldSettings,
        transport: clock::TransportState,
    ) {
        let (left_input, left_output, left_in_place) = split_channel(left);
        let (right_input, right_output, right_in_place) = split_channel(right);

        let frames = min_len(&[
            left_input.map(|buf| buf.len()),
            right_input.map(|buf| buf.len()),
            left_output.as_ref().map(|buf| buf.len()),
            right_output.as_ref().map(|buf| buf.len()),
        ]);
        let Some(frames) = frames else {
            return;
        };

        self.ensure_scratch(frames);
        for frame in 0..frames {
            self.scratch_left[frame] = if left_in_place {
                left_output
                    .as_deref()
                    .and_then(|buf| buf.get(frame))
                    .copied()
                    .unwrap_or(0.0)
            } else {
                left_input
                    .and_then(|buf| buf.get(frame))
                    .copied()
                    .unwrap_or(0.0)
            };

            self.scratch_right[frame] = if right_in_place {
                right_output
                    .as_deref()
                    .and_then(|buf| buf.get(frame))
                    .copied()
                    .unwrap_or(0.0)
            } else {
                right_input
                    .and_then(|buf| buf.get(frame))
                    .copied()
                    .unwrap_or(0.0)
            };
        }

        let report = self.engine.render(
            settings,
            &mut self.scratch_left[..frames],
            &mut self.scratch_right[..frames],
            transport,
        );
        self.shared.status.update(report);

        if let Some(out_left) = left_output {
            out_left[..frames].copy_from_slice(&self.scratch_left[..frames]);
        }
        if let Some(out_right) = right_output {
            out_right[..frames].copy_from_slice(&self.scratch_right[..frames]);
        }
    }

    fn ensure_scratch(&mut self, frames: usize) {
        if self.scratch_left.len() < frames {
            self.scratch_left.resize(frames, 0.0);
        }
        if self.scratch_right.len() < frames {
            self.scratch_right.resize(frames, 0.0);
        }
    }
}

impl PluginAudioProcessorParams for TensionFieldAudioProcessor<'_> {
    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        output_parameter_changes: &mut OutputEvents,
    ) {
        apply_param_events(input_parameter_changes, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });
        let _ = self
            .automation_drain
            .drain(&self.shared.automation_queue, output_parameter_changes);
    }
}

fn transport_state_from_transport(transport: Option<TransportEvent>) -> clock::TransportState {
    match transport {
        Some(event) => clock::TransportState {
            tempo_bpm: if event.flags.contains(TransportFlags::HAS_TEMPO) {
                event.tempo as f32
            } else {
                120.0
            },
            is_playing: event.flags.contains(TransportFlags::IS_PLAYING),
            song_pos_beats: if event.flags.contains(TransportFlags::HAS_BEATS_TIMELINE) {
                Some(event.song_pos_beats.to_float())
            } else {
                None
            },
        },
        None => clock::TransportState::default(),
    }
}

fn split_channel<'a>(
    pair: ChannelPair<'a, f32>,
) -> (Option<&'a [f32]>, Option<&'a mut [f32]>, bool) {
    match pair {
        ChannelPair::InputOnly(input) => (Some(input), None, false),
        ChannelPair::OutputOnly(output) => (None, Some(output), false),
        ChannelPair::InputOutput(input, output) => (Some(input), Some(output), false),
        ChannelPair::InPlace(output) => (None, Some(output), true),
    }
}

fn min_len(lengths: &[Option<usize>]) -> Option<usize> {
    lengths
        .iter()
        .copied()
        .flatten()
        .min()
        .filter(|len| *len > 0)
}

fn f32_to_bits(value: f32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}

fn bits_to_f32(value: u32) -> f32 {
    f32::from_ne_bytes(value.to_ne_bytes())
}

toybox::clap_plugin_entry!(TensionFieldPlugin);
