//! macOS VST3 adapter sharing Tension Field's CLAP parameters, state, DSP, and Radiant editor.

use std::ffi::c_void;

use toybox::vst3::prelude::Steinberg::*;
use toybox::vst3::prelude::*;

use crate::params::{apply_state_values, state_values};
use crate::state::{METER_COUNT, PluginStateSnapshot, STATE_MAGIC, STATE_VERSION};

mod controller;
mod factory;
mod gui_adapter;
mod param_bridge;
mod processor;
mod shared_state;
mod transport_utils;

use controller::TensionFieldVst3Controller;
use factory::TensionFieldVst3Factory;
use processor::TensionFieldVst3Processor;
use shared_state::TensionFieldVst3Shared;

/// Stable processor class identifier.
pub(super) const PROCESSOR_CID: TUID = uid(0x2EC929EB, 0xC689442E, 0xAB947965, 0xB90831A5);
/// Stable edit-controller class identifier.
pub(super) const CONTROLLER_CID: TUID = uid(0xFDE5C35C, 0xA54544FB, 0xA1B76DF2, 0x9147ABFB);
/// Current VST3 state envelope version.
pub(super) const VST3_STATE_VERSION: u32 = STATE_VERSION;
/// VST3 accepts the current state and the previous CLAP-compatible snapshot version.
pub(super) const ACCEPTED_STATE_VERSIONS: &[u32] = &[2, 3];

/// Serialize the exact existing CLAP snapshot body into a VST3 envelope.
unsafe fn write_vst3_state(stream: *mut IBStream, shared: &TensionFieldVst3Shared) -> tresult {
    let snapshot = PluginStateSnapshot {
        param_values: state_values(&shared.params),
        meter_values: shared.status.snapshot(),
    };
    let mut raw = Vec::with_capacity(16 + (crate::params::STATE_VALUE_COUNT + METER_COUNT) * 4);
    if crate::state::write_snapshot(&mut raw, &snapshot).is_err() || raw.len() < 8 {
        return kResultFalse;
    }
    match unsafe { write_versioned_payload(stream, STATE_MAGIC, VST3_STATE_VERSION, &raw[8..]) } {
        Ok(()) => kResultOk,
        Err(_) => kResultFalse,
    }
}

/// Read a VST3 snapshot atomically before applying params/meters to shared state.
unsafe fn read_vst3_state(stream: *mut IBStream, shared: &TensionFieldVst3Shared) -> tresult {
    let Ok(versioned) =
        (unsafe { read_versioned_payload(stream, STATE_MAGIC, ACCEPTED_STATE_VERSIONS) })
    else {
        return kInvalidArgument;
    };
    let mut raw = Vec::with_capacity(8 + versioned.payload.len());
    raw.extend_from_slice(&STATE_MAGIC.to_le_bytes());
    raw.extend_from_slice(&versioned.version.to_le_bytes());
    raw.extend_from_slice(&versioned.payload);
    let mut cursor = raw.as_slice();
    let Ok(snapshot) = crate::state::read_snapshot(&mut cursor) else {
        return kInvalidArgument;
    };
    apply_state_values(&shared.params, snapshot.param_values);
    shared.status.apply_snapshot(snapshot.meter_values);
    kResultOk
}

/// Create a VST3 processor instance for the class factory.
pub(super) fn create_processor() -> Option<ComPtr<FUnknown>> {
    ComWrapper::new(TensionFieldVst3Processor::new()).to_com_ptr::<FUnknown>()
}

/// Create a VST3 controller instance for the class factory.
pub(super) fn create_controller() -> Option<ComPtr<FUnknown>> {
    ComWrapper::new(TensionFieldVst3Controller::new()).to_com_ptr::<FUnknown>()
}

/// Query a class instance for the requested VST3 interface.
pub(super) unsafe fn query_instance(
    instance: ComPtr<FUnknown>,
    iid: FIDString,
    object: *mut *mut c_void,
) -> tresult {
    let pointer = instance.as_ptr();
    unsafe { ((*(*pointer).vtbl).queryInterface)(pointer, iid as *mut TUID, object) }
}

toybox::vst3_plugin_entry!(TensionFieldVst3Factory);
