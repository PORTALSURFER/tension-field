//! VST3 edit controller and Radiant view creation.

use std::ffi::{CStr, CString};
use std::ptr;
use std::slice;

use toybox::vst3::prelude::Steinberg::*;
use toybox::vst3::prelude::*;

use crate::params::text_to_value;

use super::gui_adapter::TensionFieldVst3GuiAdapter;
use super::param_bridge;
use super::shared_state::TensionFieldVst3Shared;
use super::{read_vst3_state, write_vst3_state};

/// VST3 edit controller for Tension Field.
pub(super) struct TensionFieldVst3Controller {
    connection: InstanceConnection<TensionFieldVst3Shared>,
}

impl TensionFieldVst3Controller {
    /// Create an unconnected controller endpoint with default state.
    pub(super) fn new() -> Self {
        Self {
            connection: InstanceConnection::new(
                InstanceConnectionRole::Controller,
                TensionFieldVst3Shared::new(),
            ),
        }
    }

    fn shared(&self) -> std::sync::Arc<TensionFieldVst3Shared> {
        self.connection.shared()
    }
}

impl Class for TensionFieldVst3Controller {
    type Interfaces = (IEditController, IConnectionPoint, IToyboxSharedState);
}

toybox::impl_vst3_instance_connection!(TensionFieldVst3Controller, connection);

impl IPluginBaseTrait for TensionFieldVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for TensionFieldVst3Controller {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        let shared = self.shared();
        unsafe { read_vst3_state(state, &shared) }
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        unsafe { self.setComponentState(state) }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let shared = self.shared();
        unsafe { write_vst3_state(state, &shared) }
    }

    unsafe fn getParameterCount(&self) -> int32 {
        crate::params::param_count() as int32
    }

    unsafe fn getParameterInfo(&self, index: int32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let Some(meta) = param_bridge::info(index) else {
            return kInvalidArgument;
        };
        let info = unsafe { &mut *info };
        info.id = meta.id;
        copy_wstring(meta.title, &mut info.title);
        copy_wstring(meta.short_title, &mut info.shortTitle);
        copy_wstring(meta.units, &mut info.units);
        info.stepCount = meta.step_count;
        info.defaultNormalizedValue = meta.default_normalized;
        info.unitId = 0;
        info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate;
        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: ParamID,
        normalized: ParamValue,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let Some(plain) = param_bridge::from_normalized(id, normalized) else {
            return kInvalidArgument;
        };
        let Some(display) = param_bridge::format_value(id, plain) else {
            return kInvalidArgument;
        };
        copy_wstring(&display, unsafe { &mut *string });
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        id: ParamID,
        string: *mut TChar,
        normalized: *mut ParamValue,
    ) -> tresult {
        if normalized.is_null() || string.is_null() {
            return kInvalidArgument;
        }
        let Some(raw) = (unsafe { parse_tchar_string(string) }) else {
            return kInvalidArgument;
        };
        let Ok(raw) = CString::new(raw) else {
            return kInvalidArgument;
        };
        let Some(clap_id) = param_bridge::clap_id(id) else {
            return kInvalidArgument;
        };
        let Some(plain) = text_to_value(clap_id, raw.as_c_str()) else {
            return kInvalidArgument;
        };
        let Some(value) = param_bridge::to_normalized(id, plain) else {
            return kInvalidArgument;
        };
        unsafe { *normalized = value };
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: ParamID, normalized: ParamValue) -> ParamValue {
        param_bridge::from_normalized(id, normalized).unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: ParamID, plain: ParamValue) -> ParamValue {
        param_bridge::to_normalized(id, plain).unwrap_or(0.0)
    }

    unsafe fn getParamNormalized(&self, id: ParamID) -> ParamValue {
        let shared = self.shared();
        param_bridge::read_plain(&shared.params, id)
            .and_then(|plain| param_bridge::to_normalized(id, plain))
            .unwrap_or(0.0)
    }

    unsafe fn setParamNormalized(&self, id: ParamID, normalized: ParamValue) -> tresult {
        let shared = self.shared();
        if param_bridge::apply_normalized(&shared.params, id, normalized) {
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        let shared = self.shared();
        let Ok(mut slot) = shared.component_handler.lock() else {
            return kResultFalse;
        };
        if handler.is_null() {
            *slot = None;
            return kResultOk;
        }
        unsafe { ((*(*handler).vtbl).base.addRef)(handler.cast()) };
        *slot = unsafe { ComPtr::from_raw(handler) };
        kResultOk
    }

    unsafe fn createView(&self, name: FIDString) -> *mut IPlugView {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = name;
            return ptr::null_mut();
        }

        #[cfg(target_os = "macos")]
        {
            if name.is_null() {
                return ptr::null_mut();
            }
            let requested = unsafe { CStr::from_ptr(name) };
            let editor = unsafe { CStr::from_ptr(ViewType::kEditor) };
            if requested.to_bytes() != editor.to_bytes() {
                return ptr::null_mut();
            }

            let shared = self.shared();
            let adapter = TensionFieldVst3GuiAdapter::new(shared);
            let view =
                HostedVst3View::new(adapter, crate::gui::WINDOW_WIDTH, crate::gui::WINDOW_HEIGHT)
                    .with_size_bounds(
                        crate::gui::MIN_WINDOW_WIDTH,
                        crate::gui::MIN_WINDOW_HEIGHT,
                        crate::gui::MAX_WINDOW_WIDTH,
                        crate::gui::MAX_WINDOW_HEIGHT,
                    );
            let Some(view) = ComWrapper::new(view).to_com_ptr::<IPlugView>() else {
                return ptr::null_mut();
            };
            ComPtr::into_raw(view)
        }
    }
}

unsafe fn parse_tchar_string(string: *mut TChar) -> Option<String> {
    if string.is_null() {
        return None;
    }
    let length = unsafe { tchar_len(string) };
    let utf16 = unsafe { slice::from_raw_parts(string.cast::<u16>(), length) };
    String::from_utf16(utf16).ok()
}
