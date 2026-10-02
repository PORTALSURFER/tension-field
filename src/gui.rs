//! Radiant editor for the macOS-only Tension Field CLAP and VST3 views.
//!
//! The editor deliberately keeps the audio-facing parameter and automation
//! objects owned by the plug-in. Radiant owns only the retained view and
//! forwards UI gestures back through the existing `AutomationQueue`.

use std::sync::Arc;
use std::time::Instant;

use radiant::gui::types::{Rect, Vector2};
use radiant::layout::LayoutOutput;
use radiant::prelude::{
    IntoView, KnobMessage, button, column, custom_widget_mapped, knob, row, text, toggle,
};
use radiant::runtime::PaintPrimitive;
use radiant::runtime::{DeclarativeSurfaceRuntime, Event, SurfacePaintPlan, UiSurface};
use radiant::theme::ThemeTokens;
use radiant::widgets::{
    ButtonMessage, ButtonWidget, DragHandleMessage, PointerButton, Widget, WidgetCapabilities,
    WidgetCommon, WidgetInput, WidgetKey, WidgetOutput, WidgetSizing,
};
use toybox::clack_plugin::utils::ClapId;
use toybox::clap::automation::{AutomationConfig, AutomationQueue};

use crate::params::{
    PARAM_AIR_COMP_ID, PARAM_AIR_DAMPING_ID, PARAM_CLEAN_DIRTY_ID, PARAM_DIFFUSION_ID,
    PARAM_DUCKING_ID, PARAM_ELASTICITY_ID, PARAM_ENERGY_CEILING_ID, PARAM_FEEDBACK_ID,
    PARAM_GRAIN_CONTINUITY_ID, PARAM_HOLD_ID, PARAM_MOD_A_DEPTH_ID, PARAM_MOD_A_DIVISION_ID,
    PARAM_MOD_A_RATE_HZ_ID, PARAM_MOD_A_RATE_MODE_ID, PARAM_MOD_A_SHAPE_ID,
    PARAM_MOD_A_TO_DIRECTION_ID, PARAM_MOD_A_TO_FEEDBACK_ID, PARAM_MOD_A_TO_GRAIN_ID,
    PARAM_MOD_A_TO_TENSION_ID, PARAM_MOD_A_TO_WARP_MOTION_ID, PARAM_MOD_A_TO_WIDTH_ID,
    PARAM_MOD_B_DEPTH_ID, PARAM_MOD_B_DIVISION_ID, PARAM_MOD_B_RATE_HZ_ID,
    PARAM_MOD_B_RATE_MODE_ID, PARAM_MOD_B_SHAPE_ID, PARAM_MOD_B_TO_DIRECTION_ID,
    PARAM_MOD_B_TO_FEEDBACK_ID, PARAM_MOD_B_TO_GRAIN_ID, PARAM_MOD_B_TO_TENSION_ID,
    PARAM_MOD_B_TO_WARP_MOTION_ID, PARAM_MOD_B_TO_WIDTH_ID, PARAM_MOD_RUN_ID,
    PARAM_OUTPUT_TRIM_DB_ID, PARAM_PITCH_COUPLING_ID, PARAM_PULL_DIRECTION_ID,
    PARAM_PULL_DIVISION_ID, PARAM_PULL_LATCH_ID, PARAM_PULL_QUANTIZE_ID, PARAM_PULL_RATE_ID,
    PARAM_PULL_SHAPE_ID, PARAM_PULL_TRIGGER_ID, PARAM_REBOUND_ID, PARAM_RELEASE_SNAP_ID,
    PARAM_SWING_ID, PARAM_TENSION_BIAS_ID, PARAM_TENSION_ID, PARAM_TIME_MODE_ID,
    PARAM_WARP_COLOR_ID, PARAM_WARP_MOTION_ID, PARAM_WIDTH_ID,
};
use crate::{GuiStatus, HostParamRequester};

/// Format-neutral host automation sink used by the retained editor.
///
/// The CLAP editor leaves this unset and uses its existing
/// `AutomationQueue`/flush path. VST3 supplies a sink that forwards the same
/// ordered gesture phases to `IComponentHandler`.
pub(crate) trait HostParamEditSink: Send + Sync {
    /// Begin a host gesture for one parameter.
    fn gesture_started(&self, config: &AutomationConfig, param_id: ClapId);
    /// Deliver one plain parameter value during a host gesture.
    fn gesture_value(&self, config: &AutomationConfig, param_id: ClapId, value: f64);
    /// End a host gesture for one parameter.
    fn gesture_ended(&self, config: &AutomationConfig, param_id: ClapId);
}

/// Preferred logical width of the retained editor.
pub(crate) const WINDOW_WIDTH: u32 = 1280;
/// Preferred logical height of the retained editor.
pub(crate) const WINDOW_HEIGHT: u32 = 860;
pub(crate) const MIN_WINDOW_WIDTH: u32 = 960;
pub(crate) const MIN_WINDOW_HEIGHT: u32 = 640;
pub(crate) const MAX_WINDOW_WIDTH: u32 = 1920;
pub(crate) const MAX_WINDOW_HEIGHT: u32 = 1280;
const MAP_WIDTH: f32 = 620.0;
const MAP_HEIGHT: f32 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum TensionMapOutput {
    Drag(DragHandleMessage),
}

#[derive(Clone, Debug)]
struct TensionMapWidget {
    button: ButtonWidget,
    gesture_started: bool,
}

impl TensionMapWidget {
    fn new() -> Self {
        Self {
            button: ButtonWidget::new(
                0,
                "TENSION MAP  ·  drag / double-click reset",
                WidgetSizing::fixed(Vector2::new(MAP_WIDTH, MAP_HEIGHT)),
            )
            .with_drag(),
            gesture_started: false,
        }
    }
}

fn map_point(bounds: Rect, point: radiant::gui::types::Point) -> radiant::gui::types::Point {
    radiant::gui::types::Point::new(
        ((point.x - bounds.min.x) / bounds.width().max(1.0)).clamp(0.0, 1.0),
        ((point.y - bounds.min.y) / bounds.height().max(1.0)).clamp(0.0, 1.0),
    )
}

fn localize_map_drag(bounds: Rect, drag: DragHandleMessage) -> DragHandleMessage {
    match drag {
        DragHandleMessage::Started { origin, position } => DragHandleMessage::Started {
            origin: map_point(bounds, origin),
            position: map_point(bounds, position),
        },
        DragHandleMessage::Moved { position } => DragHandleMessage::Moved {
            position: map_point(bounds, position),
        },
        DragHandleMessage::Ended { position } => DragHandleMessage::Ended {
            position: map_point(bounds, position),
        },
        DragHandleMessage::DoubleActivate { position } => DragHandleMessage::DoubleActivate {
            position: map_point(bounds, position),
        },
        DragHandleMessage::Cancelled { position } => DragHandleMessage::Cancelled {
            position: map_point(bounds, position),
        },
    }
}

impl Widget for TensionMapWidget {
    fn common(&self) -> &WidgetCommon {
        self.button.common()
    }

    fn common_mut(&mut self) -> &mut WidgetCommon {
        self.button.common_mut()
    }

    fn handle_input(&mut self, bounds: Rect, input: WidgetInput) -> Option<WidgetOutput> {
        let pointer_press = match &input {
            WidgetInput::PointerPress {
                position,
                button: PointerButton::Primary,
                ..
            } if bounds.contains(*position) => Some(*position),
            _ => None,
        };
        let pointer_release = match &input {
            WidgetInput::PointerRelease {
                position,
                button: PointerButton::Primary,
                ..
            } => Some(*position),
            _ => None,
        };

        if let WidgetInput::PointerDoubleClick {
            position,
            button: PointerButton::Primary,
            ..
        } = &input
            && bounds.contains(*position)
        {
            self.gesture_started = false;
            return Some(WidgetOutput::typed(TensionMapOutput::Drag(
                localize_map_drag(
                    bounds,
                    DragHandleMessage::DoubleActivate {
                        position: *position,
                    },
                ),
            )));
        }

        let message = self.button.handle_input(bounds, input);

        if let Some(position) = pointer_press
            && self.button.common.state.pressed
        {
            self.gesture_started = true;
            return Some(WidgetOutput::typed(TensionMapOutput::Drag(
                localize_map_drag(bounds, DragHandleMessage::started(position)),
            )));
        }

        let message = message?;
        match message {
            ButtonMessage::Activate | ButtonMessage::ActivateWithModifiers { .. } => {
                pointer_release
                    .filter(|_| self.gesture_started)
                    .map(|position| {
                        self.gesture_started = false;
                        WidgetOutput::typed(TensionMapOutput::Drag(localize_map_drag(
                            bounds,
                            DragHandleMessage::Ended { position },
                        )))
                    })
            }
            ButtonMessage::Drag(drag) => {
                let drag = match drag {
                    DragHandleMessage::Started { position, .. } if self.gesture_started => {
                        DragHandleMessage::Moved { position }
                    }
                    DragHandleMessage::Started { origin, position } => {
                        self.gesture_started = true;
                        DragHandleMessage::Started { origin, position }
                    }
                    DragHandleMessage::Moved { position } => DragHandleMessage::Moved { position },
                    DragHandleMessage::Ended { position } => {
                        self.gesture_started = false;
                        DragHandleMessage::Ended { position }
                    }
                    DragHandleMessage::DoubleActivate { position } => {
                        self.gesture_started = false;
                        DragHandleMessage::DoubleActivate { position }
                    }
                    DragHandleMessage::Cancelled { position } => {
                        self.gesture_started = false;
                        DragHandleMessage::Cancelled { position }
                    }
                };
                Some(WidgetOutput::typed(TensionMapOutput::Drag(
                    localize_map_drag(bounds, drag),
                )))
            }
            ButtonMessage::SecondaryActivate { .. } => None,
        }
    }

    fn synchronize_from_previous(&mut self, previous: &dyn Widget) {
        let Some(previous) = previous.as_any().downcast_ref::<Self>() else {
            return;
        };
        self.button.synchronize_from_previous(&previous.button);
        self.gesture_started = previous.gesture_started;
    }

    fn capabilities(&self) -> WidgetCapabilities<'_> {
        self.button.capabilities()
    }

    fn append_paint(
        &self,
        primitives: &mut Vec<PaintPrimitive>,
        bounds: Rect,
        layout: &LayoutOutput,
        theme: &ThemeTokens,
    ) {
        self.button.append_paint(primitives, bounds, layout, theme);
    }
}

/// Return the stable preferred size used before a host negotiates its child
/// view size.
pub(crate) fn preferred_window_size() -> (u32, u32) {
    (WINDOW_WIDTH, WINDOW_HEIGHT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveTab {
    Perform,
    Rhythm,
    Tone,
    Safety,
}

impl ActiveTab {
    const ALL: [Self; 4] = [Self::Perform, Self::Rhythm, Self::Tone, Self::Safety];

    const fn title(self) -> &'static str {
        match self {
            Self::Perform => "Perform",
            Self::Rhythm => "Rhythm",
            Self::Tone => "Tone + Mod",
            Self::Safety => "Safety + Out",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ParamRange {
    min: f32,
    max: f32,
    default: f32,
    step: Option<f32>,
}

impl ParamRange {
    const fn new(min: f32, max: f32, default: f32) -> Self {
        Self {
            min,
            max,
            default,
            step: None,
        }
    }

    const fn stepped(min: f32, max: f32, default: f32, step: f32) -> Self {
        Self {
            min,
            max,
            default,
            step: Some(step),
        }
    }

    fn normalize(self, value: f32) -> f32 {
        ((value - self.min) / (self.max - self.min).max(f32::EPSILON)).clamp(0.0, 1.0)
    }

    fn denormalize(self, value: f32) -> f32 {
        let value = self.min + value.clamp(0.0, 1.0) * (self.max - self.min);
        match self.step {
            Some(step) if step > 0.0 => {
                (self.min + ((value - self.min) / step).round() * step).clamp(self.min, self.max)
            }
            _ => value,
        }
    }
}

const UNIT: ParamRange = ParamRange::new(0.0, 1.0, 0.5);
const BIPOLAR: ParamRange = ParamRange::new(-1.0, 1.0, 0.0);
const PULL_RATE: ParamRange = ParamRange::new(0.02, 4.0, 0.25);
const MOD_RATE: ParamRange = ParamRange::new(0.01, 4.0, 0.25);
const FEEDBACK: ParamRange = ParamRange::new(0.0, 0.7, 0.12);
const OUTPUT_TRIM: ParamRange = ParamRange::new(-12.0, 6.0, 0.0);

#[derive(Clone, Copy, Debug)]
enum EditorMessage {
    SelectTab(ActiveTab),
    Parameter {
        id: ClapId,
        range: ParamRange,
        event: KnobMessage,
    },
    Toggle {
        id: ClapId,
        checked: bool,
    },
    Pull,
    TensionMapReset,
    TensionMapDrag(DragHandleMessage),
    Preset(usize),
}

#[derive(Clone)]
struct EditorState {
    params: Arc<crate::params::TensionFieldParams>,
    automation_queue: Arc<AutomationQueue>,
    automation_config: AutomationConfig,
    status: Arc<GuiStatus>,
    param_requester: Option<HostParamRequester>,
    edit_sink: Option<Arc<dyn HostParamEditSink>>,
    active_tab: ActiveTab,
    meter_smooth: [f32; 9],
    meter_peak_hold: [f32; 9],
    last_meter_update: Instant,
}

impl EditorState {
    #[cfg(test)]
    fn new(
        params: Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
    ) -> Self {
        Self::new_with_edit_sink(params, automation_queue, status, param_requester, None)
    }

    fn new_with_edit_sink(
        params: Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
        edit_sink: Option<Arc<dyn HostParamEditSink>>,
    ) -> Self {
        Self {
            params,
            automation_queue,
            automation_config: AutomationConfig::default(),
            status,
            param_requester,
            edit_sink,
            active_tab: ActiveTab::Perform,
            meter_smooth: [0.0; 9],
            meter_peak_hold: [0.0; 9],
            last_meter_update: Instant::now(),
        }
    }

    fn update_meters(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_meter_update)
            .as_secs_f32()
            .clamp(1.0 / 240.0, 0.1);
        self.last_meter_update = now;
        for (index, raw) in self.status.snapshot().into_iter().enumerate() {
            let raw = raw.clamp(0.0, 1.0);
            let attack = 1.0 - (-dt * 14.0).exp();
            let release = 1.0 - (-dt * 6.0).exp();
            let alpha = if raw > self.meter_smooth[index] {
                attack
            } else {
                release
            };
            self.meter_smooth[index] += (raw - self.meter_smooth[index]) * alpha;
            self.meter_peak_hold[index] = self.meter_peak_hold[index]
                .max(self.meter_smooth[index])
                .max(raw);
            self.meter_peak_hold[index] =
                (self.meter_peak_hold[index] - dt * 0.45).max(self.meter_smooth[index]);
        }
    }

    fn meter_realtime_active(&self) -> bool {
        self.status.snapshot().iter().any(|value| *value > 0.0001)
            || self.meter_smooth.iter().any(|value| *value > 0.0001)
            || self.meter_peak_hold.iter().any(|value| *value > 0.0001)
    }

    fn parameter_value(&self, id: ClapId, range: ParamRange) -> f32 {
        self.params
            .get_param(id)
            .unwrap_or(range.default)
            .clamp(range.min, range.max)
    }

    fn parameter_normalized(&self, id: ClapId, range: ParamRange) -> f32 {
        range.normalize(self.parameter_value(id, range))
    }

    fn request_flush(&self) {
        if let Some(requester) = self.param_requester {
            requester.request_flush();
        }
    }

    fn begin(&self, id: ClapId) {
        if let Some(sink) = self.edit_sink.as_ref() {
            sink.gesture_started(&self.automation_config, id);
            return;
        }
        self.automation_queue
            .push_gesture_begin(&self.automation_config, id);
        self.request_flush();
    }

    fn value(&self, id: ClapId, value: f32) {
        let value = self.canonical_value(id, value);
        if let Some(sink) = self.edit_sink.as_ref() {
            sink.gesture_value(&self.automation_config, id, value as f64);
            return;
        }
        self.automation_queue
            .push_value(&self.automation_config, id, value as f64);
        self.request_flush();
    }

    fn canonical_value(&self, id: ClapId, value: f32) -> f32 {
        self.params.set_param(id, value);
        self.params.get_param(id).unwrap_or(value)
    }

    fn end(&self, id: ClapId) {
        if let Some(sink) = self.edit_sink.as_ref() {
            sink.gesture_ended(&self.automation_config, id);
            return;
        }
        self.automation_queue
            .push_gesture_end(&self.automation_config, id);
        self.request_flush();
    }

    fn gesture(&self, id: ClapId, range: ParamRange, event: KnobMessage) {
        match event {
            KnobMessage::GestureStarted { value } => {
                self.begin(id);
                self.value(id, range.denormalize(value));
            }
            KnobMessage::ValueChanged { value } => self.value(id, range.denormalize(value)),
            KnobMessage::GestureEnded { value } => {
                self.value(id, range.denormalize(value));
                self.end(id);
            }
            KnobMessage::Reset { value } => {
                self.begin(id);
                self.value(id, range.denormalize(value));
                self.end(id);
            }
            KnobMessage::KeyboardGesture(batch) => self.apply_batch(id, range, batch.events),
            KnobMessage::WheelGesture(batch) => self.apply_batch(id, range, batch.events),
        }
    }

    fn apply_batch(
        &self,
        id: ClapId,
        range: ParamRange,
        events: [radiant::widgets::KnobAutomationEvent; 3],
    ) {
        for event in events {
            match event {
                radiant::widgets::KnobAutomationEvent::GestureStarted { value } => {
                    self.begin(id);
                    self.value(id, range.denormalize(value));
                }
                radiant::widgets::KnobAutomationEvent::ValueChanged { value } => {
                    self.value(id, range.denormalize(value));
                }
                radiant::widgets::KnobAutomationEvent::GestureEnded { value } => {
                    self.value(id, range.denormalize(value));
                    self.end(id);
                }
            }
        }
    }

    fn toggle_value(&self, id: ClapId, checked: bool) {
        let value = if checked { 1.0 } else { 0.0 };
        self.begin(id);
        self.value(id, value);
        self.end(id);
    }

    fn pull(&self) {
        self.begin(PARAM_PULL_TRIGGER_ID);
        self.value(PARAM_PULL_TRIGGER_ID, 1.0);
        self.value(PARAM_PULL_TRIGGER_ID, 0.0);
        self.end(PARAM_PULL_TRIGGER_ID);
    }

    fn paired_map_reset(&self) {
        self.begin(PARAM_PULL_DIRECTION_ID);
        self.begin(PARAM_ELASTICITY_ID);
        self.value(PARAM_PULL_DIRECTION_ID, 0.5);
        self.value(PARAM_ELASTICITY_ID, 0.65);
        self.end(PARAM_PULL_DIRECTION_ID);
        self.end(PARAM_ELASTICITY_ID);
    }

    fn paired_map_drag(&self, drag: DragHandleMessage) {
        match drag {
            DragHandleMessage::Started { position, .. } => {
                self.begin(PARAM_PULL_DIRECTION_ID);
                self.begin(PARAM_ELASTICITY_ID);
                self.map_value(position);
            }
            DragHandleMessage::Moved { position } | DragHandleMessage::Ended { position } => {
                self.map_value(position);
                if matches!(drag, DragHandleMessage::Ended { .. }) {
                    self.end(PARAM_PULL_DIRECTION_ID);
                    self.end(PARAM_ELASTICITY_ID);
                }
            }
            DragHandleMessage::DoubleActivate { .. } => self.paired_map_reset(),
            DragHandleMessage::Cancelled { .. } => {
                self.end(PARAM_PULL_DIRECTION_ID);
                self.end(PARAM_ELASTICITY_ID);
            }
        }
    }

    fn map_value(&self, position: radiant::gui::types::Point) {
        let direction = position.x.clamp(0.0, 1.0);
        let elasticity = (1.0 - position.y).clamp(0.0, 1.0);
        self.value(PARAM_PULL_DIRECTION_ID, direction);
        self.value(PARAM_ELASTICITY_ID, elasticity);
    }

    fn apply_preset(&self, index: usize) {
        for &(param_id, value) in preset_updates(index) {
            self.begin(param_id);
            self.value(param_id, value);
            self.end(param_id);
        }
    }
}

fn preset_updates(index: usize) -> &'static [(ClapId, f32)] {
    match index {
        0 => &[
            (PARAM_TENSION_ID, 0.74),
            (PARAM_PULL_SHAPE_ID, 4.0),
            (PARAM_PULL_DIVISION_ID, 4.0),
            (PARAM_PULL_QUANTIZE_ID, 1.0),
            (PARAM_TENSION_BIAS_ID, 0.75),
            (PARAM_RELEASE_SNAP_ID, 0.62),
            (PARAM_WARP_MOTION_ID, 0.56),
            (PARAM_FEEDBACK_ID, 0.26),
        ],
        1 => &[
            (PARAM_TENSION_ID, 0.67),
            (PARAM_PULL_SHAPE_ID, 2.0),
            (PARAM_PULL_DIVISION_ID, 2.0),
            (PARAM_TENSION_BIAS_ID, 0.64),
            (PARAM_GRAIN_CONTINUITY_ID, 0.54),
            (PARAM_WARP_MOTION_ID, 0.58),
            (PARAM_CLEAN_DIRTY_ID, 1.0),
        ],
        2 => &[
            (PARAM_TENSION_ID, 0.78),
            (PARAM_PULL_DIVISION_ID, 6.0),
            (PARAM_PULL_LATCH_ID, 1.0),
            (PARAM_TENSION_BIAS_ID, 0.82),
            (PARAM_RELEASE_SNAP_ID, 0.74),
            (PARAM_FEEDBACK_ID, 0.34),
            (PARAM_DUCKING_ID, 0.32),
        ],
        3 => &[
            (PARAM_TENSION_ID, 0.72),
            (PARAM_PULL_DIVISION_ID, 5.0),
            (PARAM_SWING_ID, 0.18),
            (PARAM_ELASTICITY_ID, 0.82),
            (PARAM_WARP_MOTION_ID, 0.51),
            (PARAM_DIFFUSION_ID, 0.64),
        ],
        4 => &[
            (PARAM_TENSION_ID, 0.70),
            (PARAM_PULL_DIRECTION_ID, 0.84),
            (PARAM_TENSION_BIAS_ID, 0.69),
            (PARAM_PULL_QUANTIZE_ID, 2.0),
            (PARAM_RELEASE_SNAP_ID, 0.58),
            (PARAM_WARP_COLOR_ID, 1.0),
        ],
        5 => &[
            (PARAM_TENSION_ID, 0.65),
            (PARAM_PULL_DIVISION_ID, 3.0),
            (PARAM_PULL_QUANTIZE_ID, 1.0),
            (PARAM_SWING_ID, 0.22),
            (PARAM_TENSION_BIAS_ID, 0.60),
            (PARAM_WARP_MOTION_ID, 0.61),
        ],
        6 => &[
            (PARAM_TENSION_ID, 0.52),
            (PARAM_PULL_SHAPE_ID, 0.0),
            (PARAM_PULL_DIVISION_ID, 4.0),
            (PARAM_RELEASE_SNAP_ID, 0.44),
            (PARAM_DIFFUSION_ID, 0.66),
            (PARAM_WIDTH_ID, 0.75),
        ],
        7 => &[
            (PARAM_TENSION_ID, 0.73),
            (PARAM_CLEAN_DIRTY_ID, 2.0),
            (PARAM_GRAIN_CONTINUITY_ID, 0.57),
            (PARAM_WARP_MOTION_ID, 0.67),
            (PARAM_FEEDBACK_ID, 0.22),
            (PARAM_ENERGY_CEILING_ID, 0.52),
        ],
        8 => &[
            (PARAM_TENSION_ID, 0.64),
            (PARAM_WIDTH_ID, 0.90),
            (PARAM_DIFFUSION_ID, 0.74),
            (PARAM_PULL_DIVISION_ID, 2.0),
            (PARAM_TENSION_BIAS_ID, 0.58),
            (PARAM_FEEDBACK_ID, 0.18),
        ],
        9 => &[
            (PARAM_TENSION_ID, 0.68),
            (PARAM_PULL_LATCH_ID, 1.0),
            (PARAM_PULL_DIVISION_ID, 5.0),
            (PARAM_FEEDBACK_ID, 0.41),
            (PARAM_DUCKING_ID, 0.38),
            (PARAM_ENERGY_CEILING_ID, 0.66),
        ],
        _ => &[],
    }
}

type EditorRuntime = DeclarativeSurfaceRuntime<
    EditorState,
    EditorMessage,
    fn(&mut EditorState) -> Arc<UiSurface<EditorMessage>>,
    fn(&mut EditorState, EditorMessage),
>;

/// Retained Radiant editor hosted by Toybox's macOS CLAP facade.
pub(crate) struct TensionFieldEditor {
    runtime: EditorRuntime,
    theme: ThemeTokens,
    paint_plan: SurfacePaintPlan,
    viewport: Vector2,
}

impl TensionFieldEditor {
    fn new(
        params: Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
    ) -> Self {
        Self::new_with_edit_sink(params, automation_queue, status, param_requester, None)
    }

    pub(crate) fn new_with_edit_sink(
        params: Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
        edit_sink: Option<Arc<dyn HostParamEditSink>>,
    ) -> Self {
        let theme = ThemeTokens::default();
        let viewport = Vector2::new(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32);
        Self {
            runtime: EditorRuntime::new_declarative(
                EditorState::new_with_edit_sink(
                    params,
                    automation_queue,
                    status,
                    param_requester,
                    edit_sink,
                ),
                viewport,
                project_editor_surface,
                reduce_editor_message,
            ),
            paint_plan: SurfacePaintPlan::empty(&theme),
            theme,
            viewport,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Vector2::new(width.max(1) as f32, height.max(1) as f32);
        self.runtime.dispatch_event(Event::resize(self.viewport));
    }

    fn dispatch_event(&mut self, event: Event) {
        let _ = self.runtime.dispatch_event(event);
    }

    fn paint_plan(&mut self) -> &SurfacePaintPlan {
        if self.needs_realtime_redraw() {
            self.runtime.refresh();
        }
        let _ = self
            .runtime
            .borrowed_frame_into(&self.theme, &mut self.paint_plan);
        &self.paint_plan
    }

    fn needs_realtime_redraw(&self) -> bool {
        self.runtime.bridge().state().meter_realtime_active()
    }

    fn dispatch_key_press(&mut self, key: WidgetKey) -> bool {
        self.runtime.dispatch_event(Event::key_press(key)).is_some()
    }

    fn dispatch_character(&mut self, character: char) -> bool {
        self.runtime
            .dispatch_event(Event::character(character))
            .is_some()
    }
}

impl toybox::radiant_gui::RadiantEditor for TensionFieldEditor {
    fn resize(&mut self, width: u32, height: u32) {
        Self::resize(self, width, height);
    }

    fn dispatch_event(&mut self, event: Event) {
        Self::dispatch_event(self, event);
    }

    fn paint_plan(&mut self) -> &SurfacePaintPlan {
        Self::paint_plan(self)
    }

    fn needs_realtime_redraw(&self) -> bool {
        Self::needs_realtime_redraw(self)
    }

    fn dispatch_key_press(&mut self, key: WidgetKey) -> bool {
        Self::dispatch_key_press(self, key)
    }

    fn dispatch_character(&mut self, character: char) -> bool {
        Self::dispatch_character(self, character)
    }

    fn cancel_text_entry(&mut self) -> bool {
        false
    }
}

/// Construct the retained host wrapper used by the CLAP main thread.
pub(crate) fn new_gui(
    params: Arc<crate::params::TensionFieldParams>,
    automation_queue: Arc<AutomationQueue>,
    status: Arc<GuiStatus>,
    param_requester: Option<HostParamRequester>,
) -> toybox::radiant_gui::RadiantHostedGui {
    toybox::radiant_gui::RadiantHostedGui::new(
        "TensionFieldRadiantClapEditorView",
        TensionFieldEditor::new(params, automation_queue, status, param_requester),
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
    )
    .with_size_contract(
        (MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT),
        (WINDOW_WIDTH, WINDOW_HEIGHT),
        (MAX_WINDOW_WIDTH, MAX_WINDOW_HEIGHT),
    )
}

#[allow(clippy::arc_with_non_send_sync)]
fn project_editor_surface(state: &mut EditorState) -> Arc<UiSurface<EditorMessage>> {
    state.update_meters();
    let mut tabs = Vec::with_capacity(ActiveTab::ALL.len());
    for tab in ActiveTab::ALL {
        tabs.push(
            button(tab.title())
                .selected(state.active_tab == tab)
                .message(EditorMessage::SelectTab(tab))
                .key(format!("tab-{}", tab.title())),
        );
    }

    let view = column([
        text("TENSION FIELD  ·  rhythmic strain engine")
            .key("title")
            .height(30.0),
        row(tabs).height(34.0),
        tab_surface(state),
        text("Stage meters  ·  In L  In R  Elastic  Warp  Space  Feed  Out L  Out R  Tension")
            .key("meters")
            .height(28.0),
        text("Radiant macOS editor  ·  host automation remains ordered begin → value → end")
            .key("footer")
            .height(24.0),
    ])
    .padding(16.0)
    .fill_width()
    .fill_height();

    Arc::new(view.into_surface())
}

fn tab_surface(state: &mut EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    let active_tab = state.active_tab;
    let content = match active_tab {
        ActiveTab::Perform => perform_surface(&*state),
        ActiveTab::Rhythm => rhythm_surface(&*state),
        ActiveTab::Tone => tone_surface(&*state),
        ActiveTab::Safety => safety_surface(state),
    };
    column([text(active_tab.title()).height(28.0), content])
        .key("active-tab")
        .fill_width()
        .fill_height()
}

fn perform_surface(state: &EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    let mut presets = Vec::with_capacity(10);
    for (index, label) in [
        "Pulse Drive",
        "Ratchet Pressure",
        "Pre-Drop Coil",
        "Elastic Surge",
        "Forward Strain",
        "Triplet Anxiety",
        "Ghost Lift",
        "Crush Squeeze",
        "Wide Panic",
        "Aftershock Tail",
    ]
    .into_iter()
    .enumerate()
    {
        presets.push(button(label).message(EditorMessage::Preset(index)));
    }

    column([
        row([
            param_knob(state, "Tension", PARAM_TENSION_ID, UNIT),
            param_knob(state, "Tension Bias", PARAM_TENSION_BIAS_ID, UNIT),
            param_knob(state, "Direction", PARAM_PULL_DIRECTION_ID, UNIT),
            param_knob(state, "Elasticity", PARAM_ELASTICITY_ID, UNIT),
            toggle_control(state, "Pull Latch", PARAM_PULL_LATCH_ID),
        ]),
        row([
            button("PULL").primary().message(EditorMessage::Pull),
            custom_widget_mapped(TensionMapWidget::new(), |message| {
                let TensionMapOutput::Drag(drag) = message;
                EditorMessage::TensionMapDrag(drag)
            })
            .size(MAP_WIDTH, MAP_HEIGHT),
            button("Map Reset").message(EditorMessage::TensionMapReset),
        ]),
        text("Tension Bank  ·  ten production presets").height(24.0),
        row(presets),
    ])
    .padding(10.0)
    .fill_width()
}

fn rhythm_surface(state: &EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    column([
        row([
            param_knob(state, "Pull Rate", PARAM_PULL_RATE_ID, PULL_RATE),
            param_knob(
                state,
                "Pull Shape",
                PARAM_PULL_SHAPE_ID,
                ParamRange::stepped(0.0, 4.0, 1.0, 1.0),
            ),
            param_knob(
                state,
                "Pull Division",
                PARAM_PULL_DIVISION_ID,
                ParamRange::stepped(0.0, 7.0, 4.0, 1.0),
            ),
            param_knob(state, "Swing", PARAM_SWING_ID, UNIT),
            param_knob(state, "Rebound", PARAM_REBOUND_ID, UNIT),
            param_knob(state, "Release Snap", PARAM_RELEASE_SNAP_ID, UNIT),
        ]),
        row([
            toggle_control(state, "Hold", PARAM_HOLD_ID),
            toggle_control(state, "Time Mode", PARAM_TIME_MODE_ID),
            param_knob(
                state,
                "Pull Quantize",
                PARAM_PULL_QUANTIZE_ID,
                ParamRange::stepped(0.0, 3.0, 1.0, 1.0),
            ),
        ]),
        text("Rhythm controls retain the original free-Hz, synced-division, swing, latch, and pull gestures."),
    ])
    .padding(10.0)
    .fill_width()
}

fn tone_surface(state: &EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    column([
        row([
            param_knob(state, "Grain", PARAM_GRAIN_CONTINUITY_ID, UNIT),
            param_knob(state, "Pitch Coupling", PARAM_PITCH_COUPLING_ID, UNIT),
            param_knob(state, "Width", PARAM_WIDTH_ID, UNIT),
            param_knob(state, "Diffusion", PARAM_DIFFUSION_ID, UNIT),
            param_knob(state, "Air Damping", PARAM_AIR_DAMPING_ID, UNIT),
            toggle_control(state, "Air Comp", PARAM_AIR_COMP_ID),
        ]),
        row([
            param_knob(state, "Warp Motion", PARAM_WARP_MOTION_ID, UNIT),
            param_knob(
                state,
                "Warp Color",
                PARAM_WARP_COLOR_ID,
                ParamRange::stepped(0.0, 2.0, 0.0, 1.0),
            ),
            param_knob(
                state,
                "Character",
                PARAM_CLEAN_DIRTY_ID,
                ParamRange::stepped(0.0, 2.0, 0.0, 1.0),
            ),
            param_knob(state, "Feedback", PARAM_FEEDBACK_ID, FEEDBACK),
        ]),
        text("DSP Mod Matrix").height(26.0),
        row([
            toggle_control(state, "Mod Run", PARAM_MOD_RUN_ID),
            param_knob(
                state,
                "A Shape",
                PARAM_MOD_A_SHAPE_ID,
                ParamRange::stepped(0.0, 3.0, 0.0, 1.0),
            ),
            param_knob(
                state,
                "A Mode",
                PARAM_MOD_A_RATE_MODE_ID,
                ParamRange::stepped(0.0, 1.0, 1.0, 1.0),
            ),
            param_knob(state, "A Rate", PARAM_MOD_A_RATE_HZ_ID, MOD_RATE),
            param_knob(
                state,
                "A Division",
                PARAM_MOD_A_DIVISION_ID,
                ParamRange::stepped(0.0, 7.0, 4.0, 1.0),
            ),
            param_knob(state, "A Depth", PARAM_MOD_A_DEPTH_ID, UNIT),
        ]),
        row([
            param_knob(
                state,
                "B Shape",
                PARAM_MOD_B_SHAPE_ID,
                ParamRange::stepped(0.0, 3.0, 0.0, 1.0),
            ),
            param_knob(
                state,
                "B Mode",
                PARAM_MOD_B_RATE_MODE_ID,
                ParamRange::stepped(0.0, 1.0, 1.0, 1.0),
            ),
            param_knob(state, "B Rate", PARAM_MOD_B_RATE_HZ_ID, MOD_RATE),
            param_knob(
                state,
                "B Division",
                PARAM_MOD_B_DIVISION_ID,
                ParamRange::stepped(0.0, 7.0, 4.0, 1.0),
            ),
            param_knob(state, "B Depth", PARAM_MOD_B_DEPTH_ID, UNIT),
        ]),
        text("Routes A/B  ·  Tension  Direction  Grain  Width  Warp Motion  Feedback"),
        route_row(
            state,
            "A",
            [
                PARAM_MOD_A_TO_TENSION_ID,
                PARAM_MOD_A_TO_DIRECTION_ID,
                PARAM_MOD_A_TO_GRAIN_ID,
                PARAM_MOD_A_TO_WIDTH_ID,
                PARAM_MOD_A_TO_WARP_MOTION_ID,
                PARAM_MOD_A_TO_FEEDBACK_ID,
            ],
        ),
        route_row(
            state,
            "B",
            [
                PARAM_MOD_B_TO_TENSION_ID,
                PARAM_MOD_B_TO_DIRECTION_ID,
                PARAM_MOD_B_TO_GRAIN_ID,
                PARAM_MOD_B_TO_WIDTH_ID,
                PARAM_MOD_B_TO_WARP_MOTION_ID,
                PARAM_MOD_B_TO_FEEDBACK_ID,
            ],
        ),
    ])
    .padding(10.0)
    .fill_width()
}

fn safety_surface(state: &mut EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    column([
        row([
            param_knob(state, "Ducking", PARAM_DUCKING_ID, UNIT),
            param_knob(state, "Energy Ceiling", PARAM_ENERGY_CEILING_ID, UNIT),
            param_knob(state, "Output Trim", PARAM_OUTPUT_TRIM_DB_ID, OUTPUT_TRIM),
            param_knob(state, "Air Comp", PARAM_AIR_COMP_ID, UNIT),
        ]),
        text("Stage meters  ·  In L  In R  Elastic  Warp  Space  Feed  Out L  Out R  Tension")
            .height(28.0),
        meter_row(state),
        text("Safety ceilings are always active; lower Energy Ceiling for stricter containment."),
    ])
    .padding(10.0)
    .fill_width()
}

fn route_row(
    state: &EditorState,
    label: &'static str,
    ids: [ClapId; 6],
) -> radiant::prelude::ViewNode<EditorMessage> {
    let labels = ["Ten", "Dir", "Grn", "Wid", "Warp", "Feed"];
    let mut children = vec![text(label).width(24.0)];
    for (label, id) in labels.into_iter().zip(ids) {
        children.push(param_knob(state, label, id, BIPOLAR));
    }
    row(children)
}

fn meter_row(state: &EditorState) -> radiant::prelude::ViewNode<EditorMessage> {
    let labels = [
        "In L", "In R", "Elastic", "Warp", "Space", "Feed", "Out L", "Out R", "Tension",
    ];
    let values = state.meter_smooth;
    let peaks = state.meter_peak_hold;
    let mut children = Vec::with_capacity(labels.len());
    for ((label, value), peak) in labels.into_iter().zip(values).zip(peaks) {
        children.push(
            text(format!(
                "{label}  {:.2}  ▲{:.2}",
                value.clamp(0.0, 1.0),
                peak.clamp(0.0, 1.0)
            ))
            .width(112.0),
        );
    }
    row(children)
}

fn param_knob(
    state: &EditorState,
    label: &'static str,
    id: ClapId,
    range: ParamRange,
) -> radiant::prelude::ViewNode<EditorMessage> {
    knob(state.parameter_normalized(id, range))
        .default_value(range.normalize(range.default))
        .diameter(72.0)
        .message(move |event| EditorMessage::Parameter { id, range, event })
        .key(label)
}

fn toggle_control(
    state: &EditorState,
    label: &'static str,
    id: ClapId,
) -> radiant::prelude::ViewNode<EditorMessage> {
    toggle(label, state.parameter_value(id, UNIT) >= 0.5)
        .message(move |checked| EditorMessage::Toggle { id, checked })
        .key(label)
}

fn reduce_editor_message(state: &mut EditorState, message: EditorMessage) {
    match message {
        EditorMessage::SelectTab(tab) => state.active_tab = tab,
        EditorMessage::Parameter { id, range, event } => state.gesture(id, range, event),
        EditorMessage::Toggle { id, checked } => state.toggle_value(id, checked),
        EditorMessage::Pull => state.pull(),
        EditorMessage::TensionMapReset => state.paired_map_reset(),
        EditorMessage::TensionMapDrag(drag) => state.paired_map_drag(drag),
        EditorMessage::Preset(index) => state.apply_preset(index),
    }
}

#[cfg(all(test, feature = "screenshot-test"))]
mod screenshot_tests {
    use super::*;
    use image::{ColorType, ImageFormat};
    use radiant::theme::DpiScale;
    use std::path::PathBuf;

    #[test]
    fn screenshot_renders_initial_ui() {
        let params = Arc::new(crate::params::TensionFieldParams::new());
        let status = Arc::new(GuiStatus::default());
        let mut state =
            EditorState::new(params, Arc::new(AutomationQueue::default()), status, None);
        let plan = project_editor_surface(&mut state)
            .frame_at_size(
                Vector2::new(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32),
                &ThemeTokens::default(),
            )
            .paint_plan;
        let mut capture = toybox::radiant_gui::bundled_offscreen_capture(
            Vector2::new(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32),
            DpiScale::ONE,
        )
        .expect("Radiant offscreen capture should be available");
        let pixels = capture.capture(&plan).expect("screenshot should render");
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-screenshots/tension_field");
        std::fs::create_dir_all(&root).expect("screenshot directory should be writable");
        image::save_buffer_with_format(
            root.join("initial-ui-default.png"),
            &pixels,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            ColorType::Rgba8,
            ImageFormat::Png,
        )
        .expect("screenshot should be written");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_table_keeps_complete_multi_parameter_programs() {
        assert!(preset_updates(0).len() >= 8);
        assert!(
            preset_updates(2)
                .iter()
                .any(|(id, _)| *id == PARAM_DUCKING_ID)
        );
        assert!(
            preset_updates(9)
                .iter()
                .any(|(id, _)| *id == PARAM_ENERGY_CEILING_ID)
        );
    }

    #[test]
    fn map_coordinates_are_normalized_to_the_widget_bounds() {
        let bounds = Rect::from_xy_size(42.0, 96.0, MAP_WIDTH, MAP_HEIGHT);
        let lower_left = map_point(bounds, radiant::gui::types::Point::new(42.0, 256.0));
        let upper_right = map_point(bounds, radiant::gui::types::Point::new(662.0, 96.0));
        assert_eq!(lower_left, radiant::gui::types::Point::new(0.0, 1.0));
        assert_eq!(upper_right, radiant::gui::types::Point::new(1.0, 0.0));
    }

    #[test]
    fn map_click_places_at_pointer_and_double_click_resets() {
        let bounds = Rect::from_xy_size(42.0, 96.0, MAP_WIDTH, MAP_HEIGHT);
        let position = radiant::gui::types::Point::new(352.0, 176.0);
        let local = radiant::gui::types::Point::new(0.5, 0.5);
        let mut map = TensionMapWidget::new();

        let output = Widget::handle_input(&mut map, bounds, WidgetInput::primary_press(position))
            .expect("map press should start placement");
        assert_eq!(
            output.typed_copied::<TensionMapOutput>(),
            Some(TensionMapOutput::Drag(DragHandleMessage::started(local)))
        );

        let output = Widget::handle_input(&mut map, bounds, WidgetInput::primary_release(position))
            .expect("map release should end placement");
        assert_eq!(
            output.typed_copied::<TensionMapOutput>(),
            Some(TensionMapOutput::Drag(DragHandleMessage::ended(local)))
        );

        let output = Widget::handle_input(
            &mut map,
            bounds,
            WidgetInput::primary_double_click(position),
        )
        .expect("map double-click should reset");
        assert_eq!(
            output.typed_copied::<TensionMapOutput>(),
            Some(TensionMapOutput::Drag(DragHandleMessage::double_activate(
                local
            )))
        );
    }

    #[test]
    fn stepped_parameter_values_are_canonical_before_automation() {
        let state = EditorState::new(
            Arc::new(crate::params::TensionFieldParams::new()),
            Arc::new(AutomationQueue::default()),
            Arc::new(GuiStatus::default()),
            None,
        );

        assert_eq!(state.canonical_value(PARAM_PULL_SHAPE_ID, 2.5), 3.0);
        assert_eq!(state.canonical_value(PARAM_PULL_QUANTIZE_ID, 1.5), 2.0);
        assert_eq!(state.canonical_value(PARAM_PULL_RATE_ID, 0.0), 0.02);
        assert_eq!(
            ParamRange::stepped(0.0, 4.0, 1.0, 1.0).denormalize(0.65),
            3.0
        );
    }
}
