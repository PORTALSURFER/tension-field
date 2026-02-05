//! Tension Field CLAP effect.
//!
//! Tension Field applies a dual time-warp treatment designed for slow, elastic
//! motion. The effect combines a variable-speed elastic buffer with a spectral
//! drag stage and a spatial diffusion stage for wide atmospheric textures.

#![deny(missing_docs, warnings)]

use std::sync::Arc;

use toybox::clack_common::plugin::features as plugin_features;
use toybox::clack_extensions::audio_ports::*;
use toybox::clack_extensions::params::*;
use toybox::clack_plugin::prelude::*;
use toybox::clap::params::apply_param_events;

mod dsp;
mod params;

use dsp::TensionFieldEngine;
use params::{TensionFieldParams, param_count, text_to_value, value_to_text, write_param_info};

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
            .register::<PluginParams>();
    }
}

impl DefaultPluginFactory for TensionFieldPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("com.portalsurfer.tensionfield", "Tension Field")
            .with_vendor("Portalsurfer")
            .with_version("0.1.0")
            .with_description("Elastic stretch and spectral drag audio effect")
            .with_features([plugin_features::AUDIO_EFFECT, plugin_features::STEREO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
        Ok(TensionFieldShared {
            params: Arc::new(TensionFieldParams::new()),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        Ok(TensionFieldMainThread { shared })
    }
}

/// Shared state between threads.
pub struct TensionFieldShared {
    /// Parameter storage shared between main and audio threads.
    params: Arc<TensionFieldParams>,
}

impl PluginShared<'_> for TensionFieldShared {}

/// Main-thread state for host interaction.
pub struct TensionFieldMainThread<'a> {
    shared: &'a TensionFieldShared,
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
        _output_parameter_changes: &mut OutputEvents,
    ) {
        apply_param_events(input_parameter_changes, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });
    }
}

/// Audio-thread processor for Tension Field.
pub struct TensionFieldAudioProcessor<'a> {
    shared: &'a TensionFieldShared,
    engine: TensionFieldEngine,
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
        })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        apply_param_events(events.input, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });

        let settings = self.shared.params.settings();
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
            process_stereo(&mut self.engine, left_pair, right_pair, &settings);
        }

        Ok(ProcessStatus::Continue)
    }
}

impl PluginAudioProcessorParams for TensionFieldAudioProcessor<'_> {
    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        apply_param_events(input_parameter_changes, |param_id, value| {
            self.shared.params.set_param(param_id, value as f32);
        });
    }
}

fn process_stereo(
    engine: &mut TensionFieldEngine,
    left: ChannelPair<'_, f32>,
    right: ChannelPair<'_, f32>,
    settings: &params::TensionFieldSettings,
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

    let mut scratch_left = vec![0.0_f32; frames];
    let mut scratch_right = vec![0.0_f32; frames];

    for frame in 0..frames {
        scratch_left[frame] = if left_in_place {
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

        scratch_right[frame] = if right_in_place {
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

    engine.render(settings, &mut scratch_left, &mut scratch_right);

    let mut left_output = left_output;
    let mut right_output = right_output;
    if let Some(out_left) = left_output.as_deref_mut() {
        out_left[..frames].copy_from_slice(&scratch_left[..frames]);
    }
    if let Some(out_right) = right_output.as_deref_mut() {
        out_right[..frames].copy_from_slice(&scratch_right[..frames]);
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

toybox::clap_plugin_entry!(TensionFieldPlugin);
