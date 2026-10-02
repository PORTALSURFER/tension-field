//! Radiant and component-handler bridge for the macOS VST3 editor.

use std::sync::Arc;

use toybox::clack_plugin::utils::ClapId;
use toybox::clap::automation::{AutomationConfig, AutomationQueue};
use toybox::vst3::prelude::Steinberg::Vst::IComponentHandler;
use toybox::vst3::prelude::Steinberg::{char16, int16};
use toybox::vst3::prelude::*;

use crate::gui::HostParamEditSink;

use super::param_bridge;
use super::shared_state::TensionFieldVst3Shared;

/// VST3 UI automation sink backed by the host component handler.
pub(super) struct Vst3HostParamEditSink {
    /// Pair state containing the current host component handler.
    pub(super) shared: Arc<TensionFieldVst3Shared>,
}

impl Vst3HostParamEditSink {
    fn handler(&self) -> Option<ComPtr<IComponentHandler>> {
        let guard = self.shared.component_handler.lock().ok()?;
        let handler = guard.as_ref()?;
        let pointer = handler.as_ptr();
        // Clone the COM reference while the Rust mutex is held, then release
        // the lock before re-entering the host through the handler callback.
        unsafe { ((*(*pointer).vtbl).base.addRef)(pointer.cast()) };
        Some(unsafe { ComPtr::from_raw(pointer)? })
    }

    fn enabled(config: &AutomationConfig, param_id: ClapId) -> bool {
        config.is_enabled(param_id)
    }
}

impl HostParamEditSink for Vst3HostParamEditSink {
    fn gesture_started(&self, config: &AutomationConfig, param_id: ClapId) {
        if !Self::enabled(config, param_id) {
            return;
        }
        let Some(handler) = self.handler() else {
            return;
        };
        unsafe {
            let _ = handler.beginEdit(param_id.get());
        }
    }

    fn gesture_value(&self, config: &AutomationConfig, param_id: ClapId, value: f64) {
        if !Self::enabled(config, param_id) {
            return;
        }
        let Some(normalized) = param_bridge::to_normalized(param_id.get(), value) else {
            return;
        };
        let Some(handler) = self.handler() else {
            return;
        };
        unsafe {
            let _ = handler.performEdit(param_id.get(), normalized);
        }
    }

    fn gesture_ended(&self, config: &AutomationConfig, param_id: ClapId) {
        if !Self::enabled(config, param_id) {
            return;
        }
        let Some(handler) = self.handler() else {
            return;
        };
        unsafe {
            let _ = handler.endEdit(param_id.get());
        }
    }
}

/// Adapter implementing Toybox's host-neutral VST3 Radiant view contract.
pub(super) struct TensionFieldVst3GuiAdapter {
    gui: toybox::radiant_gui::RadiantHostedGui,
}

impl TensionFieldVst3GuiAdapter {
    /// Construct the editor using the controller/processor's shared state.
    pub(super) fn new(shared: Arc<TensionFieldVst3Shared>) -> Self {
        let sink = Arc::new(Vst3HostParamEditSink {
            shared: Arc::clone(&shared),
        });
        let editor = crate::gui::TensionFieldEditor::new_with_edit_sink(
            Arc::clone(&shared.params),
            Arc::new(AutomationQueue::default()),
            Arc::clone(&shared.status),
            None,
            Some(sink),
        );
        let gui = toybox::radiant_gui::RadiantHostedGui::new(
            "TensionFieldRadiantVst3EditorView",
            editor,
            crate::gui::WINDOW_WIDTH,
            crate::gui::WINDOW_HEIGHT,
        )
        .with_size_contract(
            (crate::gui::MIN_WINDOW_WIDTH, crate::gui::MIN_WINDOW_HEIGHT),
            (crate::gui::WINDOW_WIDTH, crate::gui::WINDOW_HEIGHT),
            (crate::gui::MAX_WINDOW_WIDTH, crate::gui::MAX_WINDOW_HEIGHT),
        );
        Self { gui }
    }
}

impl Vst3HostedGui for TensionFieldVst3GuiAdapter {
    fn set_parent_raw(&mut self, parent: toybox::raw_window_handle::RawWindowHandle) {
        self.gui.set_parent(parent);
    }

    fn open(&mut self) -> bool {
        self.gui.open()
    }

    fn close(&mut self) {
        self.gui.close();
    }

    fn last_size(&self) -> Option<(u32, u32)> {
        self.gui.last_size()
    }

    fn request_resize(&self, width: u32, height: u32) {
        self.gui.request_resize(width, height);
    }

    fn on_key_down(&self, key: char16, key_code: int16, modifiers: int16) -> bool {
        self.gui.on_key_down(key, key_code, modifiers)
    }

    fn on_key_up(&self, key: char16, key_code: int16, modifiers: int16) -> bool {
        self.gui.on_key_up(key, key_code, modifiers)
    }
}
