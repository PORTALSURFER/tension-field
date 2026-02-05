//! Layout-driven performance UI for the Tension Field plugin.

use std::sync::Arc;
use std::time::Instant;

use toybox::clack_extensions::gui::Window;
use toybox::clack_plugin::plugin::PluginError;
use toybox::clack_plugin::utils::ClapId;
use toybox::clap::automation::{AutomationConfig, AutomationQueue};
use toybox::clap::gui::GuiHostWindow;
use toybox::gui::declarative::{
    Align, ButtonEvent, ButtonSpec, DeclarativeGridSpec, DropdownEvent, DropdownSpec, FlexSpec,
    KnobEvent, KnobSpec, LabelSpec, Node, Padding, PanelSpec, RegionSpec, RootFrameSpec, SizeSpec,
    ToggleEvent, ToggleSpec, UiSpec, WidgetSpec, measure,
};
use toybox::gui::{Color, Point, Rect, Size, Theme};
use toybox::patchbay_gui::Ui;
use toybox::raw_window_handle::HasRawWindowHandle;

use crate::params::{
    PARAM_AIR_COMP_ID, PARAM_AIR_DAMPING_ID, PARAM_CLEAN_DIRTY_ID, PARAM_DIFFUSION_ID,
    PARAM_ELASTICITY_ID, PARAM_FEEDBACK_ID, PARAM_GRAIN_CONTINUITY_ID, PARAM_HOLD_ID,
    PARAM_PITCH_COUPLING_ID, PARAM_PULL_DIRECTION_ID, PARAM_PULL_RATE_ID, PARAM_PULL_SHAPE_ID,
    PARAM_PULL_TRIGGER_ID, PARAM_REBOUND_ID, PARAM_TENSION_ID, PARAM_WIDTH_ID, PULL_SHAPE_LABELS,
    TensionFieldParams, pull_shape_value_from_index,
};
use crate::{GuiStatus, HostParamRequester};

const ROOT_PADDING_X: i32 = 14;
const ROOT_PADDING_Y: i32 = 12;
const PANEL_GAP: i32 = 12;
const CONTROL_GAP: i32 = 8;
const BUTTON_WIDTH: u32 = 120;
const BUTTON_HEIGHT: u32 = 24;
const TOGGLE_W: u32 = 58;
const TOGGLE_H: u32 = 18;
const DROPDOWN_W: u32 = 160;
const DROPDOWN_H: u32 = 22;
const MAP_WIDTH: u32 = 560;
const MAP_HEIGHT: u32 = 430;
const METER_CELL_W: u32 = 72;
const METER_CELL_H: u32 = 96;

const BG: Color = Color::rgb(17, 21, 28);
const PANEL_BG: Color = Color::rgb(26, 31, 40);
const PANEL_BORDER: Color = Color::rgb(58, 67, 82);
const TITLE: Color = Color::rgb(220, 225, 236);
const SUBTITLE: Color = Color::rgb(134, 150, 178);
const ACCENT: Color = Color::rgb(235, 192, 120);
const MAP_LINE: Color = Color::rgb(98, 182, 255);
const MAP_TRACE: Color = Color::rgba(132, 201, 255, 120);
const MAP_DOT: Color = Color::rgb(247, 217, 143);
const METER_FILL: Color = Color::rgb(99, 210, 188);
const METER_WARN: Color = Color::rgb(228, 148, 112);

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
        params: &Arc<TensionFieldParams>,
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
    params: &Arc<TensionFieldParams>,
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

struct GuiState {
    params: Arc<TensionFieldParams>,
    automation_queue: Arc<AutomationQueue>,
    automation_config: AutomationConfig,
    status: Arc<GuiStatus>,
    param_requester: Option<HostParamRequester>,
    map_dragging: bool,
    map_trace: Vec<Point>,
    active_mode: ModeCard,
    meter_smooth: [f32; 9],
    mod_bank: ModBank,
    last_frame: Instant,
    frame_dt: f32,
}

impl GuiState {
    fn new(
        params: Arc<TensionFieldParams>,
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
            map_dragging: false,
            map_trace: Vec::with_capacity(48),
            active_mode: ModeCard::TapeTugPad,
            meter_smooth: [0.0; 9],
            mod_bank: ModBank::default(),
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
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;
        self.frame_dt = dt;
        self.apply_modulation(dt);

        let header = Node::Widget(WidgetSpec {
            key: "tension-field-header".to_string(),
            size: SizeSpec::Fixed(Size {
                width: 420,
                height: 24,
            }),
            render: Box::new(|ui, rect, _state: &mut GuiState| {
                ui.canvas().fill_rect(rect, BG);
                ui.text_with_color(rect.origin, "TENSION FIELD", TITLE);
                ui.text_with_color(
                    Point {
                        x: rect.origin.x + 190,
                        y: rect.origin.y,
                    },
                    "elastic time warp",
                    SUBTITLE,
                );
            }),
        });

        let main_row = Node::Row(FlexSpec {
            size: SizeSpec::Auto,
            gap: PANEL_GAP,
            padding: Padding::default(),
            align: Align::Start,
            children: vec![
                self.build_gesture_panel(),
                self.build_map_panel(),
                self.build_space_mod_panel(),
            ],
        });

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
                        children: vec![header, main_row, meter_panel],
                    })),
                })),
            },
        }
    }

    fn build_gesture_panel(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "gesture-panel".to_string(),
            title: Some("Gesture".to_string()),
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
                                self.params.tension(),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.pull_button(),
                        ],
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_toggle("hold", "Hold", PARAM_HOLD_ID, self.params.hold()),
                            self.pull_shape_dropdown(),
                        ],
                    }),
                    Node::Row(FlexSpec {
                        size: SizeSpec::Auto,
                        gap: CONTROL_GAP,
                        padding: Padding::default(),
                        align: Align::Start,
                        children: vec![
                            self.param_knob(
                                "pull-rate",
                                "Pull Rate",
                                PARAM_PULL_RATE_ID,
                                self.params.pull_rate_hz(),
                                (0.02, 2.0),
                                "Hz",
                            ),
                            self.param_knob(
                                "rebound",
                                "Rebound",
                                PARAM_REBOUND_ID,
                                self.params.rebound(),
                                (0.0, 1.0),
                                "%",
                            ),
                        ],
                    }),
                    Node::Label(LabelSpec {
                        text: "Modes".to_string(),
                        size: SizeSpec::Auto,
                        color: Some(SUBTITLE),
                    }),
                    self.mode_button(ModeCard::TapeTugPad),
                    self.mode_button(ModeCard::ElasticDroneMaker),
                    self.mode_button(ModeCard::RatchetAtmos),
                    self.mode_button(ModeCard::GhostPercTail),
                ],
            })),
        })
    }

    fn build_map_panel(&self) -> Node<'static, GuiState> {
        Node::Panel(PanelSpec {
            key: "map-panel".to_string(),
            title: Some("Tension Map".to_string()),
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
                                "grain",
                                "Grain",
                                PARAM_GRAIN_CONTINUITY_ID,
                                self.params.grain_continuity(),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "pitch-coupling",
                                "Pitch Coupling",
                                PARAM_PITCH_COUPLING_ID,
                                self.params.pitch_coupling(),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "feedback",
                                "Feedback",
                                PARAM_FEEDBACK_ID,
                                self.params.feedback(),
                                (0.0, 0.6),
                                "%",
                            ),
                        ],
                    }),
                ],
            })),
        })
    }

    fn build_space_mod_panel(&self) -> Node<'static, GuiState> {
        let mod_panel = Node::Panel(PanelSpec {
            key: "mod-panel".to_string(),
            title: Some("Slow Mod Bank".to_string()),
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
                    self.mod_run_toggle(),
                    self.mod_row(
                        "A",
                        ModParam::LfoARate,
                        ModParam::LfoADepth,
                        Some(ModParam::LfoADrift),
                    ),
                    self.mod_row(
                        "B",
                        ModParam::LfoBRate,
                        ModParam::LfoBDepth,
                        Some(ModParam::LfoBDrift),
                    ),
                    self.mod_row("R", ModParam::WalkRate, ModParam::WalkDepth, None),
                    self.mod_row("E", ModParam::EnvSensitivity, ModParam::EnvDepth, None),
                    Node::Label(LabelSpec {
                        text: "Routes: Ten Dir Grn Wid".to_string(),
                        size: SizeSpec::Auto,
                        color: Some(SUBTITLE),
                    }),
                    self.mod_routes_grid(),
                ],
            })),
        });

        Node::Panel(PanelSpec {
            key: "space-panel".to_string(),
            title: Some("Space / Mod".to_string()),
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
                                "width",
                                "Width",
                                PARAM_WIDTH_ID,
                                self.params.width(),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_knob(
                                "diffusion",
                                "Diffusion",
                                PARAM_DIFFUSION_ID,
                                self.params.diffusion(),
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
                            self.param_knob(
                                "air-damping",
                                "Air Damping",
                                PARAM_AIR_DAMPING_ID,
                                self.params.air_damping(),
                                (0.0, 1.0),
                                "%",
                            ),
                            self.param_toggle(
                                "air-comp",
                                "Air Comp",
                                PARAM_AIR_COMP_ID,
                                self.params.air_compensation(),
                            ),
                            self.param_toggle(
                                "clean-dirty",
                                "Dirty",
                                PARAM_CLEAN_DIRTY_ID,
                                self.params.clean_dirty() >= 0.5,
                            ),
                        ],
                    }),
                    mod_panel,
                ],
            })),
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

    fn param_knob(
        &self,
        key: &str,
        label: &str,
        param_id: ClapId,
        value: f32,
        range: (f32, f32),
        unit: &'static str,
    ) -> Node<'static, GuiState> {
        let key_owned = key.to_string();
        let label_owned = label.to_string();
        Node::Knob(KnobSpec {
            key: key_owned,
            label: label_owned,
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
                let active = response.active || state.params.pull_trigger();
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

    fn pull_shape_dropdown(&self) -> Node<'static, GuiState> {
        Node::Dropdown(DropdownSpec {
            key: "pull-shape".to_string(),
            label: "Pull Shape".to_string(),
            options: PULL_SHAPE_LABELS.iter().map(|v| (*v).to_string()).collect(),
            selected: self.params.pull_shape_index(),
            control_size: Size {
                width: DROPDOWN_W,
                height: DROPDOWN_H,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(|state: &mut GuiState, event: DropdownEvent| {
                if event.response.changed {
                    let value = pull_shape_value_from_index(event.selected);
                    state.params.set_param(PARAM_PULL_SHAPE_ID, value);
                    state.push_begin(PARAM_PULL_SHAPE_ID);
                    state.push_value(PARAM_PULL_SHAPE_ID, value);
                    state.push_end(PARAM_PULL_SHAPE_ID);
                }
            })),
        })
    }

    fn mode_button(&self, mode: ModeCard) -> Node<'static, GuiState> {
        Node::Button(ButtonSpec {
            key: format!("mode-{}", mode.key()),
            label: format!("{}", mode.title()),
            control_size: Size {
                width: 220,
                height: 28,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(move |state: &mut GuiState, event: ButtonEvent| {
                if event.response.clicked {
                    state.apply_mode(mode);
                }
            })),
        })
    }

    fn mod_run_toggle(&self) -> Node<'static, GuiState> {
        Node::Toggle(ToggleSpec {
            key: "mod-run".to_string(),
            label: "Run".to_string(),
            value: self.mod_bank.run,
            control_size: Size {
                width: TOGGLE_W,
                height: TOGGLE_H,
            },
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(|state: &mut GuiState, event: ToggleEvent| {
                state.mod_bank.run = event.value;
            })),
        })
    }

    fn mod_row(
        &self,
        label: &str,
        rate: ModParam,
        depth: ModParam,
        drift: Option<ModParam>,
    ) -> Node<'static, GuiState> {
        let mut children = vec![
            Node::Label(LabelSpec {
                text: label.to_string(),
                size: SizeSpec::Auto,
                color: Some(TITLE),
            }),
            self.mod_knob(format!("{label}-rate"), "Rate", rate, (0.01, 1.5)),
            self.mod_knob(format!("{label}-depth"), "Depth", depth, (0.0, 1.0)),
        ];
        if let Some(drift_param) = drift {
            children.push(self.mod_knob(
                format!("{label}-drift"),
                "Drift",
                drift_param,
                (0.0, 1.0),
            ));
        }
        Node::Row(FlexSpec {
            size: SizeSpec::Auto,
            gap: CONTROL_GAP,
            padding: Padding::default(),
            align: Align::Start,
            children,
        })
    }

    fn mod_knob(
        &self,
        key: String,
        label: &'static str,
        param: ModParam,
        range: (f32, f32),
    ) -> Node<'static, GuiState> {
        let value = self.mod_bank.get(param);
        Node::Knob(KnobSpec {
            key,
            label: label.to_string(),
            value_label: Some(format!("{value:.2}")),
            value,
            range,
            size: SizeSpec::Auto,
            on_interaction: Some(Box::new(move |state: &mut GuiState, event: KnobEvent| {
                state.mod_bank.set(param, event.value);
            })),
        })
    }

    fn mod_routes_grid(&self) -> Node<'static, GuiState> {
        let mut children = Vec::with_capacity(20);
        for src in 0..4 {
            let label = match src {
                0 => "A",
                1 => "B",
                2 => "R",
                _ => "E",
            };
            children.push(Node::Label(LabelSpec {
                text: label.to_string(),
                size: SizeSpec::Auto,
                color: Some(TITLE),
            }));
            for dst in 0..4 {
                let key = format!("route-{src}-{dst}");
                let value = self.mod_bank.routes[src][dst];
                children.push(Node::Toggle(ToggleSpec {
                    key,
                    label: "".to_string(),
                    value,
                    control_size: Size {
                        width: 20,
                        height: 12,
                    },
                    size: SizeSpec::Auto,
                    on_interaction: Some(Box::new(
                        move |state: &mut GuiState, event: ToggleEvent| {
                            state.mod_bank.routes[src][dst] = event.value;
                        },
                    )),
                }));
            }
        }

        Node::Grid(DeclarativeGridSpec {
            size: SizeSpec::Auto,
            columns: 5,
            cell_size: Size {
                width: 24,
                height: 18,
            },
            gap: 4,
            padding: Padding::default(),
            children,
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
            self.set_param_immediate(PARAM_PULL_DIRECTION_ID, 0.52);
            self.set_param_immediate(PARAM_ELASTICITY_ID, 0.70);
            self.push_end(PARAM_PULL_DIRECTION_ID);
            self.push_end(PARAM_ELASTICITY_ID);
            self.map_dragging = false;
        }

        let px = rect.origin.x + (self.params.pull_direction() * rect.size.width as f32) as i32;
        let py =
            rect.origin.y + ((1.0 - self.params.elasticity()) * rect.size.height as f32) as i32;
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
        let value = self.meter_smooth[index].clamp(0.0, 1.0);

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

    fn apply_mode(&mut self, mode: ModeCard) {
        self.active_mode = mode;
        for (param_id, value) in mode.updates() {
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

    fn apply_modulation(&mut self, dt: f32) {
        if !self.mod_bank.run {
            return;
        }

        self.mod_bank.phase_a =
            (self.mod_bank.phase_a + dt * self.mod_bank.lfo_a_rate * std::f32::consts::TAU).fract();
        self.mod_bank.phase_b =
            (self.mod_bank.phase_b + dt * self.mod_bank.lfo_b_rate * std::f32::consts::TAU).fract();
        self.mod_bank.walk_state = (self.mod_bank.walk_state
            + signed_noise(&mut self.mod_bank.noise_state) * self.mod_bank.walk_rate * dt * 2.2)
            .clamp(-1.0, 1.0);

        let a = self.mod_bank.phase_a.sin() * self.mod_bank.lfo_a_depth;
        let tri = triangle(self.mod_bank.phase_b);
        let b = (tri + (self.mod_bank.phase_b.sin() * self.mod_bank.lfo_b_drift * 0.4))
            * self.mod_bank.lfo_b_depth;
        let walk = self.mod_bank.walk_state * self.mod_bank.walk_depth;

        let input = (self.status.input_left() + self.status.input_right()) * 0.5;
        self.mod_bank.env_state +=
            (input - self.mod_bank.env_state) * (0.02 + self.mod_bank.env_sensitivity * 0.3);
        let env = (self.mod_bank.env_state * 2.0 - 1.0) * self.mod_bank.env_depth;
        let sources = [a, b, walk, env];

        let mut destinations = [
            self.params.tension(),
            self.params.pull_direction(),
            self.params.grain_continuity(),
            self.params.width(),
        ];

        for (src_index, source) in sources.iter().enumerate() {
            for (dst_index, value) in destinations.iter_mut().enumerate() {
                if self.mod_bank.routes[src_index][dst_index] {
                    *value += source * 0.2;
                }
            }
        }

        let ids = [
            PARAM_TENSION_ID,
            PARAM_PULL_DIRECTION_ID,
            PARAM_GRAIN_CONTINUITY_ID,
            PARAM_WIDTH_ID,
        ];

        for (index, value) in destinations.iter_mut().enumerate() {
            *value = value.clamp(0.0, 1.0);
            if (*value - self.mod_bank.last_sent[index]).abs() > 0.002 {
                self.params.set_param(ids[index], *value);
                self.push_value(ids[index], *value);
                self.mod_bank.last_sent[index] = *value;
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ModeCard {
    TapeTugPad,
    ElasticDroneMaker,
    RatchetAtmos,
    GhostPercTail,
}

impl ModeCard {
    fn key(self) -> &'static str {
        match self {
            Self::TapeTugPad => "tape-tug",
            Self::ElasticDroneMaker => "drone-maker",
            Self::RatchetAtmos => "ratchet-atmos",
            Self::GhostPercTail => "ghost-tail",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::TapeTugPad => "Tape Tug Pad",
            Self::ElasticDroneMaker => "Elastic Drone Maker",
            Self::RatchetAtmos => "Ratchet Atmos",
            Self::GhostPercTail => "Ghost Perc Tail",
        }
    }

    fn updates(self) -> &'static [(ClapId, f32)] {
        match self {
            Self::TapeTugPad => &[
                (PARAM_TENSION_ID, 0.52),
                (PARAM_PULL_RATE_ID, 0.24),
                (PARAM_PULL_SHAPE_ID, 1.0),
                (PARAM_HOLD_ID, 0.0),
                (PARAM_GRAIN_CONTINUITY_ID, 0.2),
                (PARAM_PITCH_COUPLING_ID, 0.24),
                (PARAM_WIDTH_ID, 0.74),
                (PARAM_DIFFUSION_ID, 0.62),
                (PARAM_FEEDBACK_ID, 0.08),
            ],
            Self::ElasticDroneMaker => &[
                (PARAM_TENSION_ID, 0.68),
                (PARAM_PULL_RATE_ID, 0.12),
                (PARAM_PULL_SHAPE_ID, 1.0),
                (PARAM_HOLD_ID, 1.0),
                (PARAM_GRAIN_CONTINUITY_ID, 0.34),
                (PARAM_WIDTH_ID, 0.66),
                (PARAM_DIFFUSION_ID, 0.64),
                (PARAM_FEEDBACK_ID, 0.32),
            ],
            Self::RatchetAtmos => &[
                (PARAM_TENSION_ID, 0.58),
                (PARAM_PULL_RATE_ID, 0.34),
                (PARAM_PULL_SHAPE_ID, 2.0),
                (PARAM_HOLD_ID, 0.0),
                (PARAM_GRAIN_CONTINUITY_ID, 0.56),
                (PARAM_WIDTH_ID, 0.63),
                (PARAM_DIFFUSION_ID, 0.58),
                (PARAM_CLEAN_DIRTY_ID, 1.0),
            ],
            Self::GhostPercTail => &[
                (PARAM_TENSION_ID, 0.42),
                (PARAM_PULL_RATE_ID, 0.19),
                (PARAM_PULL_SHAPE_ID, 0.0),
                (PARAM_HOLD_ID, 0.0),
                (PARAM_GRAIN_CONTINUITY_ID, 0.16),
                (PARAM_PITCH_COUPLING_ID, 0.12),
                (PARAM_DIFFUSION_ID, 0.55),
                (PARAM_FEEDBACK_ID, 0.12),
                (PARAM_CLEAN_DIRTY_ID, 0.0),
            ],
        }
    }
}

#[derive(Copy, Clone)]
enum ModParam {
    LfoARate,
    LfoADepth,
    LfoADrift,
    LfoBRate,
    LfoBDepth,
    LfoBDrift,
    WalkRate,
    WalkDepth,
    EnvSensitivity,
    EnvDepth,
}

struct ModBank {
    run: bool,
    lfo_a_rate: f32,
    lfo_a_depth: f32,
    lfo_a_drift: f32,
    lfo_b_rate: f32,
    lfo_b_depth: f32,
    lfo_b_drift: f32,
    walk_rate: f32,
    walk_depth: f32,
    env_sensitivity: f32,
    env_depth: f32,
    routes: [[bool; 4]; 4],
    phase_a: f32,
    phase_b: f32,
    walk_state: f32,
    env_state: f32,
    last_sent: [f32; 4],
    noise_state: u32,
}

impl Default for ModBank {
    fn default() -> Self {
        Self {
            run: true,
            lfo_a_rate: 0.09,
            lfo_a_depth: 0.14,
            lfo_a_drift: 0.2,
            lfo_b_rate: 0.04,
            lfo_b_depth: 0.12,
            lfo_b_drift: 0.16,
            walk_rate: 0.25,
            walk_depth: 0.18,
            env_sensitivity: 0.46,
            env_depth: 0.12,
            routes: [
                [true, false, false, false],
                [false, true, false, false],
                [false, false, true, false],
                [false, false, false, true],
            ],
            phase_a: 0.0,
            phase_b: 0.0,
            walk_state: 0.0,
            env_state: 0.0,
            last_sent: [0.0; 4],
            noise_state: 0xA5A5_9151,
        }
    }
}

impl ModBank {
    fn get(&self, param: ModParam) -> f32 {
        match param {
            ModParam::LfoARate => self.lfo_a_rate,
            ModParam::LfoADepth => self.lfo_a_depth,
            ModParam::LfoADrift => self.lfo_a_drift,
            ModParam::LfoBRate => self.lfo_b_rate,
            ModParam::LfoBDepth => self.lfo_b_depth,
            ModParam::LfoBDrift => self.lfo_b_drift,
            ModParam::WalkRate => self.walk_rate,
            ModParam::WalkDepth => self.walk_depth,
            ModParam::EnvSensitivity => self.env_sensitivity,
            ModParam::EnvDepth => self.env_depth,
        }
    }

    fn set(&mut self, param: ModParam, value: f32) {
        match param {
            ModParam::LfoARate => self.lfo_a_rate = value,
            ModParam::LfoADepth => self.lfo_a_depth = value,
            ModParam::LfoADrift => self.lfo_a_drift = value,
            ModParam::LfoBRate => self.lfo_b_rate = value,
            ModParam::LfoBDepth => self.lfo_b_depth = value,
            ModParam::LfoBDrift => self.lfo_b_drift = value,
            ModParam::WalkRate => self.walk_rate = value,
            ModParam::WalkDepth => self.walk_depth = value,
            ModParam::EnvSensitivity => self.env_sensitivity = value,
            ModParam::EnvDepth => self.env_depth = value,
        }
    }
}

fn signed_noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*state >> 8) as f32 / ((1u32 << 24) as f32)) * 2.0 - 1.0
}

fn triangle(phase: f32) -> f32 {
    let p = phase.fract();
    if p < 0.5 {
        p * 4.0 - 1.0
    } else {
        3.0 - p * 4.0
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
        _ => format!("{value:.2}"),
    }
}
