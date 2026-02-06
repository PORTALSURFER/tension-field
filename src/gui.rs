//! Tabbed performance UI for the Tension Field plugin.

use std::sync::Arc;
use std::time::Instant;

use toybox::clack_extensions::gui::Window;
use toybox::clack_plugin::plugin::PluginError;
use toybox::clack_plugin::utils::ClapId;
use toybox::clap::automation::{AutomationConfig, AutomationQueue};
use toybox::clap::gui::GuiHostWindow;
use toybox::gui::declarative::{
    Align, ButtonEvent, ButtonSpec, DropdownEvent, DropdownSpec, FlexSpec, KnobEvent, KnobSpec,
    LabelSpec, Node, Padding, PanelSpec, RegionSpec, RootFrameSpec, SizeSpec, ToggleEvent,
    ToggleSpec, UiSpec, WidgetSpec, measure,
};
use toybox::gui::{Color, Point, Rect, Size, Theme};
use toybox::patchbay_gui::Ui;
use toybox::raw_window_handle::HasRawWindowHandle;

use crate::params::{
    CHARACTER_LABELS, MOD_RATE_MODE_LABELS, MOD_SOURCE_SHAPE_LABELS, PARAM_AIR_COMP_ID,
    PARAM_AIR_DAMPING_ID, PARAM_CLEAN_DIRTY_ID, PARAM_DIFFUSION_ID, PARAM_DUCKING_ID,
    PARAM_ELASTICITY_ID, PARAM_ENERGY_CEILING_ID, PARAM_FEEDBACK_ID, PARAM_GRAIN_CONTINUITY_ID,
    PARAM_MOD_A_DEPTH_ID, PARAM_MOD_A_DIVISION_ID, PARAM_MOD_A_RATE_HZ_ID,
    PARAM_MOD_A_RATE_MODE_ID, PARAM_MOD_A_SHAPE_ID, PARAM_MOD_A_TO_DIRECTION_ID,
    PARAM_MOD_A_TO_FEEDBACK_ID, PARAM_MOD_A_TO_GRAIN_ID, PARAM_MOD_A_TO_TENSION_ID,
    PARAM_MOD_A_TO_WARP_MOTION_ID, PARAM_MOD_A_TO_WIDTH_ID, PARAM_MOD_B_DEPTH_ID,
    PARAM_MOD_B_DIVISION_ID, PARAM_MOD_B_RATE_HZ_ID, PARAM_MOD_B_RATE_MODE_ID,
    PARAM_MOD_B_SHAPE_ID, PARAM_MOD_B_TO_DIRECTION_ID, PARAM_MOD_B_TO_FEEDBACK_ID,
    PARAM_MOD_B_TO_GRAIN_ID, PARAM_MOD_B_TO_TENSION_ID, PARAM_MOD_B_TO_WARP_MOTION_ID,
    PARAM_MOD_B_TO_WIDTH_ID, PARAM_MOD_RUN_ID, PARAM_OUTPUT_TRIM_DB_ID, PARAM_PITCH_COUPLING_ID,
    PARAM_PULL_DIRECTION_ID, PARAM_PULL_DIVISION_ID, PARAM_PULL_LATCH_ID, PARAM_PULL_QUANTIZE_ID,
    PARAM_PULL_RATE_ID, PARAM_PULL_SHAPE_ID, PARAM_PULL_TRIGGER_ID, PARAM_REBOUND_ID,
    PARAM_RELEASE_SNAP_ID, PARAM_SWING_ID, PARAM_TENSION_BIAS_ID, PARAM_TENSION_ID,
    PARAM_TIME_MODE_ID, PARAM_WARP_COLOR_ID, PARAM_WARP_MOTION_ID, PARAM_WIDTH_ID, PARAM_HOLD_ID,
    PULL_DIVISION_LABELS, PULL_QUANTIZE_LABELS, PULL_SHAPE_LABELS, TIME_MODE_LABELS,
    WARP_COLOR_LABELS, character_mode_value_from_index, mod_rate_mode_value_from_index,
    mod_source_shape_value_from_index, pull_division_value_from_index,
    pull_quantize_value_from_index, pull_shape_value_from_index, warp_color_value_from_index,
};
use crate::{GuiStatus, HostParamRequester};

const ROOT_PADDING_X: i32 = 14;
const ROOT_PADDING_Y: i32 = 12;
const PANEL_GAP: i32 = 12;
const CONTROL_GAP: i32 = 8;
const BUTTON_WIDTH: u32 = 124;
const BUTTON_HEIGHT: u32 = 24;
const TOGGLE_W: u32 = 60;
const TOGGLE_H: u32 = 18;
const DROPDOWN_W: u32 = 160;
const DROPDOWN_H: u32 = 22;
const MAP_WIDTH: u32 = 620;
const MAP_HEIGHT: u32 = 360;
const METER_CELL_W: u32 = 72;
const METER_CELL_H: u32 = 96;

const BG: Color = Color::rgb(16, 20, 26);
const PANEL_BG: Color = Color::rgb(25, 30, 39);
const PANEL_BORDER: Color = Color::rgb(58, 67, 82);
const TITLE: Color = Color::rgb(220, 225, 236);
const SUBTITLE: Color = Color::rgb(134, 150, 178);
const ACCENT: Color = Color::rgb(235, 192, 120);
const TAB_ACTIVE: Color = Color::rgb(78, 111, 170);
const TAB_INACTIVE: Color = Color::rgb(43, 51, 66);
const MAP_LINE: Color = Color::rgb(98, 182, 255);
const MAP_TRACE: Color = Color::rgba(132, 201, 255, 120);
const MAP_DOT: Color = Color::rgb(247, 217, 143);
const METER_FILL: Color = Color::rgb(99, 210, 188);
const METER_WARN: Color = Color::rgb(228, 148, 112);
const METER_HOLD: Color = Color::rgb(250, 234, 158);

/// GUI window manager for Tension Field.
#[derive(Default)]
pub struct TensionFieldGui {
    window: GuiHostWindow,
    is_open: bool,
}

impl TensionFieldGui {
    /// Attach the host parent window.
    pub fn set_parent(&mut self, window: Window<'_>) {
        self.window.set_parent(window.raw_window_handle());
    }

    /// Return the last known logical window size.
    pub fn last_size(&self) -> Option<(u32, u32)> {
        self.window.last_size()
    }

    /// Open the plugin editor with shared parameter and metering state.
    pub fn open(
        &mut self,
        params: &Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
    ) -> Result<(), PluginError> {
        if self.is_open {
            return Ok(());
        }

        let mut state = GuiState::new(
            Arc::clone(params),
            automation_queue,
            status,
            param_requester,
        );
        let (width, height) = state.measure_window_size();

        self.window.open_parented(
            "Tension Field".to_string(),
            (width, height),
            state,
            |_state| {},
            |_input, state: &mut GuiState| state.build_spec(),
        )?;
        self.is_open = true;
        Ok(())
    }

    /// Request a resize on the GUI thread.
    pub fn request_resize(&self, width: u32, height: u32) {
        self.window.request_resize(width, height);
    }

    /// Close the editor if it is currently open.
    pub fn close(&mut self) {
        self.window.hide();
        self.is_open = false;
    }
}

/// Measure the preferred editor size from the declarative layout.
pub fn preferred_window_size(
    params: &Arc<crate::params::TensionFieldParams>,
    status: &Arc<GuiStatus>,
) -> (u32, u32) {
    let mut state = GuiState::new(
        Arc::clone(params),
        Arc::new(AutomationQueue::default()),
        Arc::clone(status),
        None,
    );
    state.measure_window_size()
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ActiveTab {
    Perform,
    Rhythm,
    Tone,
    Safety,
}

impl ActiveTab {
    fn key(self) -> &'static str {
        match self {
            Self::Perform => "perform",
            Self::Rhythm => "rhythm",
            Self::Tone => "tone",
            Self::Safety => "safety",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Perform => "Perform",
            Self::Rhythm => "Rhythm",
            Self::Tone => "Tone + Mod",
            Self::Safety => "Safety + Out",
        }
    }

    fn all() -> [Self; 4] {
        [Self::Perform, Self::Rhythm, Self::Tone, Self::Safety]
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum TensionPreset {
    PulseDrive,
    RatchetPressure,
    PreDropCoil,
    ElasticSurge,
    ForwardStrain,
    TripletAnxiety,
    GhostLift,
    CrushSqueeze,
    WidePanic,
    AftershockTail,
}

impl TensionPreset {
    fn all() -> [Self; 10] {
        [
            Self::PulseDrive,
            Self::RatchetPressure,
            Self::PreDropCoil,
            Self::ElasticSurge,
            Self::ForwardStrain,
            Self::TripletAnxiety,
            Self::GhostLift,
            Self::CrushSqueeze,
            Self::WidePanic,
            Self::AftershockTail,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::PulseDrive => "Pulse Drive",
            Self::RatchetPressure => "Ratchet Pressure",
            Self::PreDropCoil => "Pre-Drop Coil",
            Self::ElasticSurge => "Elastic Surge",
            Self::ForwardStrain => "Forward Strain",
            Self::TripletAnxiety => "Triplet Anxiety",
            Self::GhostLift => "Ghost Lift",
            Self::CrushSqueeze => "Crush Squeeze",
            Self::WidePanic => "Wide Panic",
            Self::AftershockTail => "Aftershock Tail",
        }
    }

    fn updates(self) -> &'static [(ClapId, f32)] {
        match self {
            Self::PulseDrive => &[
                (PARAM_TENSION_ID, 0.74),
                (PARAM_PULL_SHAPE_ID, 4.0),
                (PARAM_PULL_DIVISION_ID, 4.0),
                (PARAM_PULL_QUANTIZE_ID, 1.0),
                (PARAM_TENSION_BIAS_ID, 0.75),
                (PARAM_RELEASE_SNAP_ID, 0.62),
                (PARAM_WARP_MOTION_ID, 0.56),
                (PARAM_FEEDBACK_ID, 0.26),
            ],
            Self::RatchetPressure => &[
                (PARAM_TENSION_ID, 0.67),
                (PARAM_PULL_SHAPE_ID, 2.0),
                (PARAM_PULL_DIVISION_ID, 2.0),
                (PARAM_TENSION_BIAS_ID, 0.64),
                (PARAM_GRAIN_CONTINUITY_ID, 0.54),
                (PARAM_WARP_MOTION_ID, 0.58),
                (PARAM_CLEAN_DIRTY_ID, 1.0),
            ],
            Self::PreDropCoil => &[
                (PARAM_TENSION_ID, 0.78),
                (PARAM_PULL_DIVISION_ID, 6.0),
                (PARAM_PULL_LATCH_ID, 1.0),
                (PARAM_TENSION_BIAS_ID, 0.82),
                (PARAM_RELEASE_SNAP_ID, 0.74),
                (PARAM_FEEDBACK_ID, 0.34),
                (PARAM_DUCKING_ID, 0.32),
            ],
            Self::ElasticSurge => &[
                (PARAM_TENSION_ID, 0.72),
                (PARAM_PULL_DIVISION_ID, 5.0),
                (PARAM_SWING_ID, 0.18),
                (PARAM_ELASTICITY_ID, 0.82),
                (PARAM_WARP_MOTION_ID, 0.51),
                (PARAM_DIFFUSION_ID, 0.64),
            ],
            Self::ForwardStrain => &[
                (PARAM_TENSION_ID, 0.7),
                (PARAM_PULL_DIRECTION_ID, 0.84),
                (PARAM_TENSION_BIAS_ID, 0.69),
                (PARAM_PULL_QUANTIZE_ID, 2.0),
                (PARAM_RELEASE_SNAP_ID, 0.58),
                (PARAM_WARP_COLOR_ID, 1.0),
            ],
            Self::TripletAnxiety => &[
                (PARAM_TENSION_ID, 0.65),
                (PARAM_PULL_DIVISION_ID, 3.0),
                (PARAM_PULL_QUANTIZE_ID, 1.0),
                (PARAM_SWING_ID, 0.22),
                (PARAM_TENSION_BIAS_ID, 0.6),
                (PARAM_WARP_MOTION_ID, 0.61),
            ],
            Self::GhostLift => &[
                (PARAM_TENSION_ID, 0.52),
                (PARAM_PULL_SHAPE_ID, 0.0),
                (PARAM_PULL_DIVISION_ID, 4.0),
                (PARAM_RELEASE_SNAP_ID, 0.44),
                (PARAM_DIFFUSION_ID, 0.66),
                (PARAM_WIDTH_ID, 0.75),
            ],
            Self::CrushSqueeze => &[
                (PARAM_TENSION_ID, 0.73),
                (PARAM_CLEAN_DIRTY_ID, 2.0),
                (PARAM_GRAIN_CONTINUITY_ID, 0.57),
                (PARAM_WARP_MOTION_ID, 0.67),
                (PARAM_FEEDBACK_ID, 0.22),
                (PARAM_ENERGY_CEILING_ID, 0.52),
            ],
            Self::WidePanic => &[
                (PARAM_TENSION_ID, 0.64),
                (PARAM_WIDTH_ID, 0.9),
                (PARAM_DIFFUSION_ID, 0.74),
                (PARAM_PULL_DIVISION_ID, 2.0),
                (PARAM_TENSION_BIAS_ID, 0.58),
                (PARAM_FEEDBACK_ID, 0.18),
            ],
            Self::AftershockTail => &[
                (PARAM_TENSION_ID, 0.68),
                (PARAM_PULL_LATCH_ID, 1.0),
                (PARAM_PULL_DIVISION_ID, 5.0),
                (PARAM_FEEDBACK_ID, 0.41),
                (PARAM_DUCKING_ID, 0.38),
                (PARAM_ENERGY_CEILING_ID, 0.66),
            ],
        }
    }
}

struct GuiState {
    params: Arc<crate::params::TensionFieldParams>,
    automation_queue: Arc<AutomationQueue>,
    automation_config: AutomationConfig,
    status: Arc<GuiStatus>,
    param_requester: Option<HostParamRequester>,
    active_tab: ActiveTab,
    map_dragging: bool,
    map_trace: Vec<Point>,
    meter_smooth: [f32; 9],
    meter_peak_hold: [f32; 9],
    last_frame: Instant,
    frame_dt: f32,
}

impl GuiState {
    fn new(
        params: Arc<crate::params::TensionFieldParams>,
        automation_queue: Arc<AutomationQueue>,
        status: Arc<GuiStatus>,
        param_requester: Option<HostParamRequester>,
    ) -> Self {
        Self {
            params,
            automation_queue,
            automation_config: AutomationConfig::default(),
            status,
            param_requester,
            active_tab: ActiveTab::Perform,
            map_dragging: false,
            map_trace: Vec::with_capacity(48),
            meter_smooth: [0.0; 9],
            meter_peak_hold: [0.0; 9],
            last_frame: Instant::now(),
            frame_dt: 1.0 / 60.0,
        }
    }

    fn measure_window_size(&mut self) -> (u32, u32) {
        let spec = self.build_spec();
        let measured = measure(&spec, &Theme::default());
        (measured.width.max(1), measured.height.max(1))
    }

    fn build_spec(&mut self) -> UiSpec<'static, GuiState> {
        let now = Instant::now();
        self.frame_dt = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;

        let header = Node::Widget(WidgetSpec {
            key: "tension-field-header".to_string(),
            size: SizeSpec::Fixed(Size {
                width: 460,
                height: 24,
            }),
            render: Box::new(|ui, rect, state: &mut GuiState| {
                ui.canvas().fill_rect(rect, BG);
                ui.text_with_color(rect.origin, "TENSION FIELD", TITLE);
                ui.text_with_color(
                    Point {
                        x: rect.origin.x + 190,
                        y: rect.origin.y,
                    },
                    "rhythmic strain engine",
                    SUBTITLE,
                );
                ui.text_with_color(
                    Point {
                        x: rect.origin.x + 370,
                        y: rect.origin.y,
                    },
                    state.active_tab.title(),
                    ACCENT,
                );
            }),
        });

        let tabs = self.build_tab_row();
        let main_content = match self.active_tab {
            ActiveTab::Perform => self.build_perform_tab(),
            ActiveTab::Rhythm => self.build_rhythm_tab(),
            ActiveTab::Tone => self.build_tone_tab(),
            ActiveTab::Safety => self.build_safety_tab(),
        };

        let meter_panel = self.build_meter_panel();

        UiSpec {
            root: RootFrameSpec {
                key: "tension-field-root".to_string(),
                title: None,
                padding: 0,
                content: Box::new(Node::Panel(PanelSpec {
                    key: "tension-field-main".to_string(),
                    title: None,
                    padding: 0,
                    background: Some(BG),
                    outline: Some(BG),
                    header_height: Some(0),
                    size: SizeSpec::Auto,
                    content: Box::new(Node::Column(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: PANEL_GAP,
                        padding: Padding::symmetric(ROOT_PADDING_X, ROOT_PADDING_Y),
                        align: Align::Start,
                        children: vec![header, tabs, main_content, meter_panel],
                    })),
                })),
            },
        }
    }

    fn build_tab_row(&self) -> Node<'static, GuiState> {
        let mut children = Vec::with_capacity(ActiveTab::all().len());
        for tab in ActiveTab::all() {
            children.push(self.tab_button(tab));
        }
        Node::Row(FlexSpec {
            size: SizeSpec::Auto,
            gap: CONTROL_GAP,
            padding: Padding::default(),
            align: Align::Start,
            children,
        })
    }

    fn tab_button(&self, tab: ActiveTab) -> Node<'static, GuiState> {
        Node::Region(RegionSpec {
            key: format!("tab-{}", tab.key()),
            size: Size {
                width: 120,
                height: 24,
            },
            on_interaction: Some(Box::new(move |state: &mut GuiState, event| {
                // Treat release-inside as a click for compatibility across toybox UI versions.
                if event.response.released && event.response.hovered {
                    state.active_tab = tab;
                }
            })),
            draw: Some(Box::new(
                move |canvas, rect, state: &mut GuiState, response| {
                    let active = state.active_tab == tab;
                    let fill = if active {
                        TAB_ACTIVE
                    } else if response.hovered {
                        Color::rgb(60, 72, 90)
                    } else {
                        TAB_INACTIVE
                    };
                    canvas.fill_rect(rect, fill);
                    canvas.stroke_rect(rect, 1, PANEL_BORDER);
                    canvas.draw_text(
                        Point {
                            x: rect.origin.x + 14,
                            y: rect.origin.y + 8,
                        },
                        tab.title(),
                        if active { BG } else { TITLE },
                        1,
                    );
                },
            )),
        })
    }

    fn build_perform_tab(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "perform-tab".to_string(),
            title: Some("Perform".to_string()),
            padding: 10,
            background: Some(PANEL_BG),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Column(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children: vec![
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "tension",
                                "Tension",
                                PARAM_TENSION_ID,
                                self.param_value(PARAM_TENSION_ID, 0.5),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "tension-bias",
                                "Tension Bias",
                                PARAM_TENSION_BIAS_ID,
                                self.param_value(PARAM_TENSION_BIAS_ID, 0.5),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.pull_button(),
                            self.param_toggle(
                                "pull-latch",
                                "Latch",
                                PARAM_PULL_LATCH_ID,
                                self.param_bool(PARAM_PULL_LATCH_ID, false),
                            ),
                            self.param_dropdown(
                                "pull-quant",
                                "Quant",
                                PARAM_PULL_QUANTIZE_ID,
                                PULL_QUANTIZE_LABELS
                                    .iter()
                                    .map(|v| (*v).to_string())
                                    .collect(),
                                self.param_value(PARAM_PULL_QUANTIZE_ID, 1.0).round() as usize,
                                pull_quantize_value_from_index,
                            ),
                        ],
                    }),
                    self.quantize_indicator(),
                    Node::Widget(WidgetSpec {
                        key: "tension-map-widget".to_string(),
                        size: SizeSpec::Fixed(Size {
                            width: MAP_WIDTH,
                            height: MAP_HEIGHT,
                        }),
                        render: Box::new(|ui, rect, state: &mut GuiState| {
                            state.draw_tension_map(ui, rect);
                        }),
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "direction",
                                "Direction",
                                PARAM_PULL_DIRECTION_ID,
                                self.param_value(PARAM_PULL_DIRECTION_ID, 0.5),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "elasticity",
                                "Elasticity",
                                PARAM_ELASTICITY_ID,
                                self.param_value(PARAM_ELASTICITY_ID, 0.65),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_dropdown(
                                "pull-shape",
                                "Pull Shape",
                                PARAM_PULL_SHAPE_ID,
                                PULL_SHAPE_LABELS.iter().map(|v| (*v).to_string()).collect(),
                                self.param_value(PARAM_PULL_SHAPE_ID, 1.0).round() as usize,
                                pull_shape_value_from_index,
                            ),
                        ],
                    }),
                    self.build_preset_bank(),
                ],
            })),
        })
    }

    fn build_rhythm_tab(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "rhythm-tab".to_string(),
            title: Some("Rhythm".to_string()),
            padding: 10,
            background: Some(PANEL_BG),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Column(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children: vec![
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_dropdown(
                                "time-mode",
                                "Time Mode",
                                PARAM_TIME_MODE_ID,
                                TIME_MODE_LABELS.iter().map(|v| (*v).to_string()).collect(),
                                self.param_value(PARAM_TIME_MODE_ID, 1.0).round() as usize,
                                |index| index.min(1) as f32,
                            ),
                            self.param_dropdown(
                                "pull-division",
                                "Pull Division",
                                PARAM_PULL_DIVISION_ID,
                                PULL_DIVISION_LABELS
                                    .iter()
                                    .map(|v| (*v).to_string())
                                    .collect(),
                                self.param_value(PARAM_PULL_DIVISION_ID, 4.0).round() as usize,
                                pull_division_value_from_index,
                            ),
                            self.param_knob(
                                "pull-rate",
                                "Pull Rate",
                                PARAM_PULL_RATE_ID,
                                self.param_value(PARAM_PULL_RATE_ID, 0.35),
                                (0.02, 4.0),
                                "Hz",
                            ),
                        ],
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "swing",
                                "Swing",
                                PARAM_SWING_ID,
                                self.param_value(PARAM_SWING_ID, 0.0),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "rebound",
                                "Rebound",
                                PARAM_REBOUND_ID,
                                self.param_value(PARAM_REBOUND_ID, 0.55),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "release-snap",
                                "Release Snap",
                                PARAM_RELEASE_SNAP_ID,
                                self.param_value(PARAM_RELEASE_SNAP_ID, 0.35),
                                (0.0, 1.0),
                                "%",
                            ),
                        ],
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_toggle(
                                "hold",
                                "Hold",
                                PARAM_HOLD_ID,
                                self.param_bool(PARAM_HOLD_ID, false),
                            ),
                            self.param_toggle(
                                "pull-latch-r",
                                "Pull Latch",
                                PARAM_PULL_LATCH_ID,
                                self.param_bool(PARAM_PULL_LATCH_ID, false),
                            ),
                            self.param_dropdown(
                                "pull-quant-r",
                                "Pull Quant",
                                PARAM_PULL_QUANTIZE_ID,
                                PULL_QUANTIZE_LABELS
                                    .iter()
                                    .map(|v| (*v).to_string())
                                    .collect(),
                                self.param_value(PARAM_PULL_QUANTIZE_ID, 1.0).round() as usize,
                                pull_quantize_value_from_index,
                            ),
                        ],
                    }),
                ],
            })),
        })
    }

    fn build_tone_tab(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "tone-tab".to_string(),
            title: Some("Tone + Mod".to_string()),
            padding: 10,
            background: Some(PANEL_BG),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Column(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children: vec![
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "grain",
                                "Grain",
                                PARAM_GRAIN_CONTINUITY_ID,
                                self.param_value(PARAM_GRAIN_CONTINUITY_ID, 0.28),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "pitch-coupling",
                                "Pitch Coupling",
                                PARAM_PITCH_COUPLING_ID,
                                self.param_value(PARAM_PITCH_COUPLING_ID, 0.2),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "warp-motion",
                                "Warp Motion",
                                PARAM_WARP_MOTION_ID,
                                self.param_value(PARAM_WARP_MOTION_ID, 0.35),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_dropdown(
                                "warp-color",
                                "Warp Color",
                                PARAM_WARP_COLOR_ID,
                                WARP_COLOR_LABELS.iter().map(|v| (*v).to_string()).collect(),
                                self.param_value(PARAM_WARP_COLOR_ID, 0.0).round() as usize,
                                warp_color_value_from_index,
                            ),
                            self.param_dropdown(
                                "character",
                                "Character",
                                PARAM_CLEAN_DIRTY_ID,
                                CHARACTER_LABELS.iter().map(|v| (*v).to_string()).collect(),
                                self.param_value(PARAM_CLEAN_DIRTY_ID, 0.0).round() as usize,
                                character_mode_value_from_index,
                            ),
                        ],
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "width",
                                "Width",
                                PARAM_WIDTH_ID,
                                self.param_value(PARAM_WIDTH_ID, 0.6),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "diffusion",
                                "Diffusion",
                                PARAM_DIFFUSION_ID,
                                self.param_value(PARAM_DIFFUSION_ID, 0.55),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "air-damping",
                                "Air Damping",
                                PARAM_AIR_DAMPING_ID,
                                self.param_value(PARAM_AIR_DAMPING_ID, 0.35),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_toggle(
                                "air-comp",
                                "Air Comp",
                                PARAM_AIR_COMP_ID,
                                self.param_bool(PARAM_AIR_COMP_ID, true),
                            ),
                        ],
                    }),
                    self.build_mod_matrix_panel(),
                ],
            })),
        })
    }

    fn build_safety_tab(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "safety-tab".to_string(),
            title: Some("Safety + Output".to_string()),
            padding: 10,
            background: Some(PANEL_BG),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Column(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children: vec![
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "feedback",
                                "Feedback",
                                PARAM_FEEDBACK_ID,
                                self.param_value(PARAM_FEEDBACK_ID, 0.12),
                                (0.0, 0.7),
                                "%",
                            ),
                            self.param_knob(
                                "ducking",
                                "Ducking",
                                PARAM_DUCKING_ID,
                                self.param_value(PARAM_DUCKING_ID, 0.0),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "energy-ceiling",
                                "Energy Ceiling",
                                PARAM_ENERGY_CEILING_ID,
                                self.param_value(PARAM_ENERGY_CEILING_ID, 0.7),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "output-trim",
                                "Output Trim",
                                PARAM_OUTPUT_TRIM_DB_ID,
                                self.param_value(PARAM_OUTPUT_TRIM_DB_ID, 0.0),
                                (-12.0, 6.0),
                                "dB",
                            ),
                        ],
                    }),
                    Node::Label(LabelSpec {
                        text: "Safety ceilings are always active; lower Energy Ceiling for stricter containment."
                            .to_string(),
                        size: SizeSpec::Auto,
                        color: Some(SUBTITLE),
                    }),
                ],
            })),
        })
    }

    fn build_mod_matrix_panel(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "mod-matrix-panel".to_string(),
            title: Some("DSP Mod Matrix".to_string()),
            padding: 8,
            background: Some(Color::rgb(21, 26, 34)),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Column(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children: vec![
                    self.param_toggle(
                        "mod-run",
                        "Run",
                        PARAM_MOD_RUN_ID,
                        self.param_bool(PARAM_MOD_RUN_ID, true),
                    ),
                    self.mod_source_row(
                        "A",
                        PARAM_MOD_A_SHAPE_ID,
                        PARAM_MOD_A_RATE_MODE_ID,
                        PARAM_MOD_A_RATE_HZ_ID,
                        PARAM_MOD_A_DIVISION_ID,
                        PARAM_MOD_A_DEPTH_ID,
                    ),
                    self.mod_source_row(
                        "B",
                        PARAM_MOD_B_SHAPE_ID,
                        PARAM_MOD_B_RATE_MODE_ID,
                        PARAM_MOD_B_RATE_HZ_ID,
                        PARAM_MOD_B_DIVISION_ID,
                        PARAM_MOD_B_DEPTH_ID,
                    ),
                    Node::Label(LabelSpec {
                        text: "Routes (A/B): Tension Direction Grain Width Warp Feedback"
                            .to_string(),
                        size: SizeSpec::Auto,
                        color: Some(SUBTITLE),
                    }),
                    self.mod_routes_row(
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
                    self.mod_routes_row(
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
                ],
            })),
        })
    }

    fn mod_source_row(
        &self,
        label: &'static str,
        shape_id: ClapId,
        rate_mode_id: ClapId,
        rate_hz_id: ClapId,
        division_id: ClapId,
        depth_id: ClapId,
    ) -> Node<'static, GuiState> {
        Node::Row(FlexSpec {
            size: SizeSpec::Auto,
            gap: CONTROL_GAP,
            padding: Padding::default(),
            align: Align::Start,
            children: vec![
                Node::Label(LabelSpec {
                    text: label.to_string(),
                    size: SizeSpec::Auto,
                    color: Some(TITLE),
                }),
                self.param_dropdown(
                    format!("mod-{label}-shape"),
                    "Shape",
                    shape_id,
                    MOD_SOURCE_SHAPE_LABELS
                        .iter()
                        .map(|v| (*v).to_string())
                        .collect(),
                    self.param_value(shape_id, 0.0).round() as usize,
                    mod_source_shape_value_from_index,
                ),
                self.param_dropdown(
                    format!("mod-{label}-rate-mode"),
                    "Rate Mode",
                    rate_mode_id,
                    MOD_RATE_MODE_LABELS
                        .iter()
                        .map(|v| (*v).to_string())
                        .collect(),
                    self.param_value(rate_mode_id, 1.0).round() as usize,
                    mod_rate_mode_value_from_index,
                ),
                self.param_knob(
                    format!("mod-{label}-rate-hz"),
                    "Rate Hz",
                    rate_hz_id,
                    self.param_value(rate_hz_id, 0.1),
                    (0.01, 4.0),
                    "Hz",
                ),
                self.param_dropdown(
                    format!("mod-{label}-division"),
                    "Division",
                    division_id,
                    PULL_DIVISION_LABELS
                        .iter()
                        .map(|v| (*v).to_string())
                        .collect(),
                    self.param_value(division_id, 4.0).round() as usize,
                    pull_division_value_from_index,
                ),
                self.param_knob(
                    format!("mod-{label}-depth"),
                    "Depth",
                    depth_id,
                    self.param_value(depth_id, 0.2),
                    (0.0, 1.0),
                    "%",
                ),
            ],
        })
    }

    fn mod_routes_row(&self, label: &'static str, ids: [ClapId; 6]) -> Node<'static, GuiState> {
        Node::Row(FlexSpec {
            size: SizeSpec::Auto,
            gap: CONTROL_GAP,
            padding: Padding::default(),
            align: Align::Start,
            children: vec![
                Node::Label(LabelSpec {
                    text: label.to_string(),
                    size: SizeSpec::Auto,
                    color: Some(TITLE),
                }),
                self.param_knob(
                    format!("route-{label}-tension"),
                    "Ten",
                    ids[0],
                    self.param_value(ids[0], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
                self.param_knob(
                    format!("route-{label}-direction"),
                    "Dir",
                    ids[1],
                    self.param_value(ids[1], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
                self.param_knob(
                    format!("route-{label}-grain"),
                    "Grn",
                    ids[2],
                    self.param_value(ids[2], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
                self.param_knob(
                    format!("route-{label}-width"),
                    "Wid",
                    ids[3],
                    self.param_value(ids[3], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
                self.param_knob(
                    format!("route-{label}-warp"),
                    "Warp",
                    ids[4],
                    self.param_value(ids[4], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
                self.param_knob(
                    format!("route-{label}-feedback"),
                    "Feed",
                    ids[5],
                    self.param_value(ids[5], 0.0),
                    (-1.0, 1.0),
                    "",
                ),
            ],
        })
    }

    fn build_preset_bank(&self) -> Node<'static, GuiState> {
        let mut children = Vec::with_capacity(TensionPreset::all().len());
        for preset in TensionPreset::all() {
            children.push(self.preset_button(preset));
        }
        Node::Panel(PanelSpec {
            key: "preset-bank".to_string(),
            title: Some("Tension Bank".to_string()),
            padding: 8,
            background: Some(Color::rgb(21, 26, 34)),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Row(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children,
            })),
        })
    }

    fn preset_button(&self, preset: TensionPreset) -> Node<'static, GuiState> {
        Node::Button(ButtonSpec {
            key: format!("preset-{:?}", preset),
            label: preset.label().to_string(),
            control_size: Size {
                width: 124,
                height: 26,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(move |state: &mut GuiState, event: ButtonEvent| {
                if event.response.clicked {
                    state.apply_preset(preset);
                }
            })),
        })
    }

    fn quantize_indicator(&self) -> Node<'static, GuiState> {
        Node::Widget(WidgetSpec {
            key: "quantize-indicator".to_string(),
            size: SizeSpec::Fixed(Size {
                width: 220,
                height: 18,
            }),
            render: Box::new(|ui, rect, state: &mut GuiState| {
                let pull = state.param_bool(PARAM_PULL_TRIGGER_ID, false);
                let latch = state.param_bool(PARAM_PULL_LATCH_ID, false);
                let text = if pull || latch {
                    "Quantize Armed"
                } else {
                    "Quantize Idle"
                };
                ui.canvas().fill_rect(rect, Color::rgb(20, 24, 31));
                ui.text_with_color(
                    rect.origin,
                    text,
                    if pull || latch { ACCENT } else { SUBTITLE },
                );
            }),
        })
    }

    fn build_meter_panel(&self) -> Node<'static, GuiState> {
        let labels = [
            "In L", "In R", "Elastic", "Warp", "Space", "Feed", "Out L", "Out R", "Tension",
        ];
        let mut children = Vec::with_capacity(labels.len());
        for (index, label) in labels.iter().enumerate() {
            let meter_index = index;
            let meter_label = (*label).to_string();
            children.push(Node::Widget(WidgetSpec {
                key: format!("meter-{meter_index}"),
                size: SizeSpec::Fixed(Size {
                    width: METER_CELL_W,
                    height: METER_CELL_H,
                }),
                render: Box::new(move |ui, rect, state: &mut GuiState| {
                    state.draw_meter_cell(ui, rect, meter_index, &meter_label);
                }),
            }));
        }

        Node::Panel(PanelSpec {
            key: "meters-panel".to_string(),
            title: Some("Stage Meters".to_string()),
            padding: 10,
            background: Some(PANEL_BG),
            outline: Some(PANEL_BORDER),
            header_height: None,
            size: SizeSpec::Auto,
            content: Box::new(Node::Row(FlexSpec {
                size: SizeSpec::Auto,
                gap: CONTROL_GAP,
                padding: Padding::default(),
                align: Align::Start,
                children,
            })),
        })
    }

    fn param_value(&self, param_id: ClapId, default: f32) -> f32 {
        self.params.get_param(param_id).unwrap_or(default)
    }

    fn param_bool(&self, param_id: ClapId, default: bool) -> bool {
        self.param_value(param_id, if default { 1.0 } else { 0.0 }) >= 0.5
    }

    fn param_knob<K: Into<String>>(
        &self,
        key: K,
        label: &str,
        param_id: ClapId,
        value: f32,
        range: (f32, f32),
        unit: &'static str,
    ) -> Node<'static, GuiState> {
        Node::Knob(KnobSpec {
            key: key.into(),
            label: label.to_string(),
            value_label: Some(format_value(value, range, unit)),
            value,
            range,
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(move |state: &mut GuiState, event: KnobEvent| {
                state.params.set_param(param_id, event.value);
                state.push_value(param_id, event.value);
            })),
        })
    }

    fn param_toggle(
        &self,
        key: &str,
        label: &str,
        param_id: ClapId,
        value: bool,
    ) -> Node<'static, GuiState> {
        Node::Toggle(ToggleSpec {
            key: key.to_string(),
            label: label.to_string(),
            value,
            control_size: Size {
                width: TOGGLE_W,
                height: TOGGLE_H,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(move |state: &mut GuiState, event: ToggleEvent| {
                let raw = if event.value { 1.0 } else { 0.0 };
                state.params.set_param(param_id, raw);
                state.push_begin(param_id);
                state.push_value(param_id, raw);
                state.push_end(param_id);
            })),
        })
    }

    fn param_dropdown<K: Into<String>>(
        &self,
        key: K,
        label: &str,
        param_id: ClapId,
        options: Vec<String>,
        selected: usize,
        value_from_index: fn(usize) -> f32,
    ) -> Node<'static, GuiState> {
        Node::Dropdown(DropdownSpec {
            key: key.into(),
            label: label.to_string(),
            options,
            selected,
            control_size: Size {
                width: DROPDOWN_W,
                height: DROPDOWN_H,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(
                move |state: &mut GuiState, event: DropdownEvent| {
                    if event.response.changed {
                        let value = value_from_index(event.selected);
                        state.params.set_param(param_id, value);
                        state.push_begin(param_id);
                        state.push_value(param_id, value);
                        state.push_end(param_id);
                    }
                },
            )),
        })
    }

    fn pull_button(&self) -> Node<'static, GuiState> {
        Node::Region(RegionSpec {
            key: "pull-button".to_string(),
            size: Size {
                width: BUTTON_WIDTH,
                height: BUTTON_HEIGHT,
            },
            on_interaction: Some(Box::new(|state: &mut GuiState, event| {
                if event.response.pressed {
                    state.push_begin(PARAM_PULL_TRIGGER_ID);
                    state.params.set_param(PARAM_PULL_TRIGGER_ID, 1.0);
                    state.push_value(PARAM_PULL_TRIGGER_ID, 1.0);
                }
                if event.response.released {
                    state.params.set_param(PARAM_PULL_TRIGGER_ID, 0.0);
                    state.push_value(PARAM_PULL_TRIGGER_ID, 0.0);
                    state.push_end(PARAM_PULL_TRIGGER_ID);
                }
            })),
            draw: Some(Box::new(|canvas, rect, state: &mut GuiState, response| {
                let active = response.active || state.param_bool(PARAM_PULL_TRIGGER_ID, false);
                let fill = if active {
                    ACCENT
                } else if response.hovered {
                    Color::rgb(62, 74, 94)
                } else {
                    Color::rgb(44, 52, 66)
                };
                canvas.fill_rect(rect, fill);
                canvas.stroke_rect(rect, 1, PANEL_BORDER);
                canvas.draw_text(
                    Point {
                        x: rect.origin.x + 38,
                        y: rect.origin.y + 8,
                    },
                    "PULL",
                    Color::rgb(12, 14, 20),
                    1,
                );
            })),
        })
    }

    fn draw_tension_map(&mut self, ui: &mut Ui<'_>, rect: Rect) {
        {
            let canvas = ui.canvas();
            canvas.fill_rect(rect, Color::rgb(22, 27, 35));
            canvas.stroke_rect(rect, 1, PANEL_BORDER);

            let center_x = rect.origin.x + rect.size.width as i32 / 2;
            let center_y = rect.origin.y + rect.size.height as i32 / 2;
            canvas.draw_line(
                Point {
                    x: center_x,
                    y: rect.origin.y,
                },
                Point {
                    x: center_x,
                    y: rect.origin.y + rect.size.height as i32,
                },
                Color::rgb(52, 62, 77),
            );
            canvas.draw_line(
                Point {
                    x: rect.origin.x,
                    y: center_y,
                },
                Point {
                    x: rect.origin.x + rect.size.width as i32,
                    y: center_y,
                },
                Color::rgb(52, 62, 77),
            );
        }

        let response = ui.region_with_key("tension-map-region", rect);
        let pointer = ui.input().pointer_pos;
        if response.pressed {
            self.map_dragging = true;
            self.push_begin(PARAM_PULL_DIRECTION_ID);
            self.push_begin(PARAM_ELASTICITY_ID);
            self.update_map_from_pointer(pointer, rect);
        }
        if response.dragged && self.map_dragging {
            self.update_map_from_pointer(pointer, rect);
        }
        if response.released && self.map_dragging {
            self.push_end(PARAM_PULL_DIRECTION_ID);
            self.push_end(PARAM_ELASTICITY_ID);
            self.map_dragging = false;
        }
        if response.double_clicked {
            self.push_begin(PARAM_PULL_DIRECTION_ID);
            self.push_begin(PARAM_ELASTICITY_ID);
            self.set_param_immediate(PARAM_PULL_DIRECTION_ID, 0.5);
            self.set_param_immediate(PARAM_ELASTICITY_ID, 0.65);
            self.push_end(PARAM_PULL_DIRECTION_ID);
            self.push_end(PARAM_ELASTICITY_ID);
            self.map_dragging = false;
        }

        let px = rect.origin.x
            + (self.param_value(PARAM_PULL_DIRECTION_ID, 0.5) * rect.size.width as f32) as i32;
        let py = rect.origin.y
            + ((1.0 - self.param_value(PARAM_ELASTICITY_ID, 0.65)) * rect.size.height as f32)
                as i32;
        let point = Point { x: px, y: py };

        self.map_trace.push(point);
        if self.map_trace.len() > 36 {
            self.map_trace.remove(0);
        }

        {
            let canvas = ui.canvas();
            for pair in self.map_trace.windows(2) {
                if let [a, b] = pair {
                    canvas.draw_line(*a, *b, MAP_TRACE);
                }
            }

            canvas.draw_line(
                Point {
                    x: px,
                    y: rect.origin.y,
                },
                Point {
                    x: px,
                    y: rect.origin.y + rect.size.height as i32,
                },
                MAP_LINE,
            );
            canvas.draw_line(
                Point {
                    x: rect.origin.x,
                    y: py,
                },
                Point {
                    x: rect.origin.x + rect.size.width as i32,
                    y: py,
                },
                MAP_LINE,
            );

            canvas.fill_circle(point, 8, MAP_DOT);
            canvas.stroke_circle(point, 12, 2, ACCENT);

            canvas.draw_text(
                Point {
                    x: rect.origin.x + 8,
                    y: rect.origin.y - 14,
                },
                "BACKWARD",
                SUBTITLE,
                1,
            );
            canvas.draw_text(
                Point {
                    x: rect.origin.x + rect.size.width as i32 - 58,
                    y: rect.origin.y - 14,
                },
                "FORWARD",
                SUBTITLE,
                1,
            );
            canvas.draw_text(
                Point {
                    x: rect.origin.x + 2,
                    y: rect.origin.y + rect.size.height as i32 + 6,
                },
                "VISCOUS",
                SUBTITLE,
                1,
            );
            canvas.draw_text(
                Point {
                    x: rect.origin.x + rect.size.width as i32 - 40,
                    y: rect.origin.y + rect.size.height as i32 + 6,
                },
                "SPRING",
                SUBTITLE,
                1,
            );
        }
    }

    fn draw_meter_cell(&mut self, ui: &mut Ui<'_>, rect: Rect, index: usize, label: &str) {
        let values = [
            self.status.input_left(),
            self.status.input_right(),
            self.status.elastic_activity(),
            self.status.warp_activity(),
            self.status.space_activity(),
            self.status.feedback_activity(),
            self.status.output_left(),
            self.status.output_right(),
            self.status.tension_activity(),
        ];

        self.meter_smooth[index] +=
            (values[index] - self.meter_smooth[index]) * (self.frame_dt * 12.0);
        self.meter_peak_hold[index] = if values[index] >= self.meter_peak_hold[index] {
            values[index]
        } else {
            (self.meter_peak_hold[index] - self.frame_dt * 0.4).max(self.meter_smooth[index])
        };

        let value = self.meter_smooth[index].clamp(0.0, 1.0);
        let hold = self.meter_peak_hold[index].clamp(0.0, 1.0);

        let bar_rect = Rect {
            origin: Point {
                x: rect.origin.x,
                y: rect.origin.y,
            },
            size: Size {
                width: rect.size.width,
                height: rect.size.height.saturating_sub(18),
            },
        };
        ui.canvas().fill_rect(bar_rect, Color::rgb(32, 37, 46));
        ui.canvas().stroke_rect(bar_rect, 1, PANEL_BORDER);

        let fill_h = (bar_rect.size.height as f32 * value).round() as u32;
        if fill_h > 0 {
            let fill_rect = Rect {
                origin: Point {
                    x: bar_rect.origin.x,
                    y: bar_rect.origin.y + bar_rect.size.height as i32 - fill_h as i32,
                },
                size: Size {
                    width: bar_rect.size.width,
                    height: fill_h,
                },
            };
            let color = if value > 0.85 { METER_WARN } else { METER_FILL };
            ui.canvas().fill_rect(fill_rect, color);
        }

        let hold_y = bar_rect.origin.y + bar_rect.size.height as i32
            - (bar_rect.size.height as f32 * hold).round() as i32;
        ui.canvas().draw_line(
            Point {
                x: bar_rect.origin.x,
                y: hold_y,
            },
            Point {
                x: bar_rect.origin.x + bar_rect.size.width as i32,
                y: hold_y,
            },
            METER_HOLD,
        );

        ui.text_with_color(
            Point {
                x: rect.origin.x,
                y: rect.origin.y + rect.size.height as i32 - 14,
            },
            label,
            SUBTITLE,
        );
    }

    fn update_map_from_pointer(&self, pointer: Point, rect: Rect) {
        let local_x = (pointer.x - rect.origin.x) as f32;
        let local_y = (pointer.y - rect.origin.y) as f32;
        let x = (local_x / rect.size.width.max(1) as f32).clamp(0.0, 1.0);
        let y = (1.0_f32 - (local_y / rect.size.height.max(1) as f32)).clamp(0.0, 1.0);
        self.params.set_param(PARAM_PULL_DIRECTION_ID, x);
        self.params.set_param(PARAM_ELASTICITY_ID, y);
        self.push_value(PARAM_PULL_DIRECTION_ID, x);
        self.push_value(PARAM_ELASTICITY_ID, y);
    }

    fn set_param_immediate(&self, param_id: ClapId, value: f32) {
        self.params.set_param(param_id, value);
        self.push_value(param_id, value);
    }

    fn apply_preset(&mut self, preset: TensionPreset) {
        for (param_id, value) in preset.updates() {
            self.push_begin(*param_id);
            self.params.set_param(*param_id, *value);
            self.push_value(*param_id, *value);
            self.push_end(*param_id);
        }
    }

    fn request_flush(&self) {
        if let Some(requester) = self.param_requester {
            requester.request_flush();
        }
    }

    fn push_value(&self, param_id: ClapId, value: f32) {
        self.automation_queue
            .push_value(&self.automation_config, param_id, value as f64);
        self.request_flush();
    }

    fn push_begin(&self, param_id: ClapId) {
        self.automation_queue
            .push_gesture_begin(&self.automation_config, param_id);
        self.request_flush();
    }

    fn push_end(&self, param_id: ClapId) {
        self.automation_queue
            .push_gesture_end(&self.automation_config, param_id);
        self.request_flush();
    }
}

fn format_value(value: f32, range: (f32, f32), unit: &'static str) -> String {
    match unit {
        "%" => {
            let span = (range.1 - range.0).max(1.0e-6);
            let pct = ((value - range.0) / span * 100.0).clamp(0.0, 100.0);
            format!("{pct:.0}%")
        }
        "Hz" => format!("{value:.2} Hz"),
        "dB" => format!("{value:+.1} dB"),
        _ => format!("{value:.2}"),
    }
}
