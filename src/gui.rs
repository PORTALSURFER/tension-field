//! Industrial performance UI for the Tension Field plugin.

use std::sync::Arc;
use std::time::Instant;

use toybox::clack_extensions::gui::Window;
use toybox::clack_plugin::plugin::PluginError;
use toybox::clack_plugin::utils::ClapId;
use toybox::clap::automation::{AutomationConfig, AutomationQueue};
use toybox::clap::gui::GuiHostWindow;
use toybox::gui::{Color, Point, Rect, Size, Ui, WidgetId};
use toybox::raw_window_handle::HasRawWindowHandle;

use crate::params::{
    PARAM_AIR_COMP_ID, PARAM_AIR_DAMPING_ID, PARAM_CLEAN_DIRTY_ID, PARAM_DIFFUSION_ID,
    PARAM_ELASTICITY_ID, PARAM_FEEDBACK_ID, PARAM_GRAIN_CONTINUITY_ID, PARAM_HOLD_ID,
    PARAM_PITCH_COUPLING_ID, PARAM_PULL_DIRECTION_ID, PARAM_PULL_RATE_ID, PARAM_PULL_SHAPE_ID,
    PARAM_PULL_TRIGGER_ID, PARAM_REBOUND_ID, PARAM_TENSION_ID, PARAM_WIDTH_ID, PULL_SHAPE_LABELS,
    TensionFieldParams, pull_shape_value_from_index,
};
use crate::{GuiStatus, HostParamRequester};

/// Default width of the Tension Field editor.
pub const WINDOW_WIDTH: u32 = 1280;
/// Default height of the Tension Field editor.
pub const WINDOW_HEIGHT: u32 = 860;

const PADDING: i32 = 18;
const TOP_Y: i32 = 66;
const PANEL_HEIGHT: i32 = 640;
const LEFT_W: i32 = 300;
const CENTER_W: i32 = 600;
const RIGHT_W: i32 = 300;
const GAP: i32 = 20;
const PANEL_TITLE_Y: i32 = 10;
const PANEL_CONTENT_Y: i32 = 40;
const KNOB_W: i32 = 78;
const KNOB_GAP_X: i32 = 20;
const KNOB_GAP_Y: i32 = 26;
const BUTTON_H: i32 = 26;
const CARD_H: i32 = 58;
const CARD_GAP: i32 = 10;
const DROPDOWN_H: i32 = 22;
const TOGGLE_H: i32 = 18;
const TOGGLE_W: i32 = 48;
const MAP_SIZE: i32 = 460;
const METER_H: i32 = 120;

const BG: Color = Color::rgb(17, 21, 28);
const PANEL_BG: Color = Color::rgb(26, 31, 40);
const PANEL_BORDER: Color = Color::rgb(58, 67, 82);
const TITLE: Color = Color::rgb(220, 225, 236);
const SUBTITLE: Color = Color::rgb(134, 150, 178);
const ACCENT: Color = Color::rgb(235, 192, 120);
const MAP_LINE: Color = Color::rgb(98, 182, 255);
const MAP_TRACE: Color = Color::rgba(132, 201, 255, 120);
const MAP_DOT: Color = Color::rgb(247, 217, 143);
const CARD_ACTIVE: Color = Color::rgb(57, 66, 82);
const CARD_IDLE: Color = Color::rgb(33, 39, 50);
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

        let state = GuiState::new(
            Arc::clone(params),
            automation_queue,
            status,
            param_requester,
        );
        self.window.open_parented(
            "Tension Field".to_string(),
            (WINDOW_WIDTH, WINDOW_HEIGHT),
            state,
            |_state| {},
            |_input, state: &mut GuiState| state.build_spec(),
        )?;
        self.is_open = true;
        Ok(())
    }

    /// Close the editor if it is currently open.
    pub fn close(&mut self) {
        self.window.hide();
        self.is_open = false;
    }
}

struct GuiState {
    params: Arc<TensionFieldParams>,
    automation_queue: Arc<AutomationQueue>,
    automation_config: AutomationConfig,
    status: Arc<GuiStatus>,
    param_requester: Option<HostParamRequester>,
    active_continuous: Option<(WidgetId, ClapId)>,
    map_dragging: bool,
    map_trace: Vec<Point>,
    active_mode: ModeCard,
    meter_smooth: [f32; 9],
    mod_bank: ModBank,
    last_frame: Instant,
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
            active_continuous: None,
            map_dragging: false,
            map_trace: Vec::with_capacity(48),
            active_mode: ModeCard::TapeTugPad,
            meter_smooth: [0.0; 9],
            mod_bank: ModBank::default(),
            last_frame: Instant::now(),
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

    fn set_param_immediate(&self, param_id: ClapId, value: f32) {
        self.params.set_param(param_id, value);
        self.push_value(param_id, value);
    }

    fn set_param_toggle(&self, param_id: ClapId, enabled: bool) {
        self.params
            .set_param(param_id, if enabled { 1.0 } else { 0.0 });
        self.push_begin(param_id);
        self.push_value(param_id, if enabled { 1.0 } else { 0.0 });
        self.push_end(param_id);
    }

    fn begin_if_needed(&mut self, id: WidgetId, param_id: ClapId, active: bool) {
        match (self.active_continuous, active) {
            (None, true) => {
                self.active_continuous = Some((id, param_id));
                self.push_begin(param_id);
            }
            (Some((current, current_param)), true) if current != id => {
                self.push_end(current_param);
                self.active_continuous = Some((id, param_id));
                self.push_begin(param_id);
            }
            (Some((current, current_param)), false) if current == id => {
                self.push_end(current_param);
                self.active_continuous = None;
            }
            _ => {}
        }
    }

    fn build_spec(&mut self) -> toybox::gui::declarative::UiSpec<'static, GuiState> {
        use toybox::gui::Theme;
        use toybox::gui::declarative::{
            Node, PanelSpec, RootFrameSpec, SizeSpec, UiSpec, WidgetSpec,
        };

        let theme = Theme::default();
        UiSpec {
            root: RootFrameSpec {
                key: "tension-field-root".to_string(),
                title: None,
                padding: 0,
                content: Box::new(Node::Panel(PanelSpec {
                    key: "tension-field-panel".to_string(),
                    title: None,
                    padding: 0,
                    background: Some(theme.background),
                    outline: Some(theme.background),
                    header_height: Some(0),
                    size: SizeSpec::Fixed(Size {
                        width: WINDOW_WIDTH,
                        height: WINDOW_HEIGHT,
                    }),
                    content: Box::new(Node::Widget(WidgetSpec {
                        key: "tension-field-widget".to_string(),
                        size: SizeSpec::Fixed(Size {
                            width: WINDOW_WIDTH,
                            height: WINDOW_HEIGHT,
                        }),
                        render: Box::new(|ui, _rect, state: &mut GuiState| {
                            state.draw_content(ui);
                        }),
                    })),
                })),
            },
        }
    }

    fn draw_content(&mut self, ui: &mut Ui<'_>) {
        ui.canvas().clear(BG);

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;

        ui.text_with_color(Point { x: PADDING, y: 18 }, "TENSION FIELD", TITLE);
        ui.text_with_color(
            Point {
                x: PADDING + 190,
                y: 18,
            },
            "elastic time warp",
            SUBTITLE,
        );

        let left_panel = Rect {
            origin: Point {
                x: PADDING,
                y: TOP_Y,
            },
            size: Size {
                width: LEFT_W as u32,
                height: PANEL_HEIGHT as u32,
            },
        };
        let center_panel = Rect {
            origin: Point {
                x: PADDING + LEFT_W + GAP,
                y: TOP_Y,
            },
            size: Size {
                width: CENTER_W as u32,
                height: PANEL_HEIGHT as u32,
            },
        };
        let right_panel = Rect {
            origin: Point {
                x: PADDING + LEFT_W + GAP + CENTER_W + GAP,
                y: TOP_Y,
            },
            size: Size {
                width: RIGHT_W as u32,
                height: PANEL_HEIGHT as u32,
            },
        };
        let meter_panel = Rect {
            origin: Point {
                x: PADDING,
                y: TOP_Y + PANEL_HEIGHT + GAP,
            },
            size: Size {
                width: (WINDOW_WIDTH as i32 - PADDING * 2) as u32,
                height: METER_H as u32,
            },
        };

        draw_panel(ui, left_panel, "Gesture");
        draw_panel(ui, center_panel, "Tension Map");
        draw_panel(ui, right_panel, "Space / Mod");
        draw_panel(ui, meter_panel, "Stage Meters");

        self.draw_left_panel(ui, left_panel);
        self.draw_center_panel(ui, center_panel);
        self.draw_right_panel(ui, right_panel);

        self.apply_modulation(dt);
        self.draw_meters(ui, meter_panel);

        ui.track_rect(Rect {
            origin: Point { x: 0, y: 0 },
            size: Size {
                width: WINDOW_WIDTH,
                height: WINDOW_HEIGHT,
            },
        });
    }

    fn draw_left_panel(&mut self, ui: &mut Ui<'_>, panel: Rect) {
        let x = panel.origin.x + 16;
        let y = panel.origin.y + PANEL_CONTENT_Y;

        self.draw_param_knob(
            ui,
            "tension",
            "Tension",
            PARAM_TENSION_ID,
            self.params.tension(),
            (0.0, 1.0),
            Point { x, y },
            &format!("{:.0}%", self.params.tension() * 100.0),
        );

        self.draw_pull_button(
            ui,
            Rect {
                origin: Point { x: x + 120, y },
                size: Size {
                    width: 110,
                    height: BUTTON_H as u32,
                },
            },
        );

        self.draw_param_toggle(
            ui,
            "hold",
            "Hold",
            PARAM_HOLD_ID,
            self.params.hold(),
            Point {
                x: x + 120,
                y: y + 42,
            },
        );

        self.draw_param_knob(
            ui,
            "pull-rate",
            "Pull Rate",
            PARAM_PULL_RATE_ID,
            self.params.pull_rate_hz(),
            (0.02, 2.0),
            Point {
                x,
                y: y + KNOB_W + KNOB_GAP_Y,
            },
            &format!("{:.2} Hz", self.params.pull_rate_hz()),
        );

        self.draw_pull_shape_dropdown(
            ui,
            Rect {
                origin: Point {
                    x: x + 120,
                    y: y + KNOB_W + KNOB_GAP_Y + 14,
                },
                size: Size {
                    width: 150,
                    height: DROPDOWN_H as u32,
                },
            },
        );

        self.draw_param_knob(
            ui,
            "rebound",
            "Rebound",
            PARAM_REBOUND_ID,
            self.params.rebound(),
            (0.0, 1.0),
            Point {
                x,
                y: y + (KNOB_W + KNOB_GAP_Y) * 2,
            },
            &format!("{:.0}%", self.params.rebound() * 100.0),
        );

        self.draw_mode_cards(
            ui,
            Point {
                x,
                y: panel.origin.y + panel.size.height as i32 - (CARD_H + CARD_GAP) * 4 - 14,
            },
            panel.size.width as i32 - 32,
        );
    }

    fn draw_center_panel(&mut self, ui: &mut Ui<'_>, panel: Rect) {
        let map_rect = Rect {
            origin: Point {
                x: panel.origin.x + (panel.size.width as i32 - MAP_SIZE) / 2,
                y: panel.origin.y + PANEL_CONTENT_Y,
            },
            size: Size {
                width: MAP_SIZE as u32,
                height: MAP_SIZE as u32,
            },
        };
        self.draw_tension_map(ui, map_rect);

        let row_y = map_rect.origin.y + MAP_SIZE + 20;
        self.draw_param_knob(
            ui,
            "grain",
            "Grain",
            PARAM_GRAIN_CONTINUITY_ID,
            self.params.grain_continuity(),
            (0.0, 1.0),
            Point {
                x: map_rect.origin.x,
                y: row_y,
            },
            &format!("{:.0}%", self.params.grain_continuity() * 100.0),
        );
        self.draw_param_knob(
            ui,
            "pitch-coupling",
            "Pitch Coupling",
            PARAM_PITCH_COUPLING_ID,
            self.params.pitch_coupling(),
            (0.0, 1.0),
            Point {
                x: map_rect.origin.x + KNOB_W + KNOB_GAP_X,
                y: row_y,
            },
            &format!("{:.0}%", self.params.pitch_coupling() * 100.0),
        );
        self.draw_param_knob(
            ui,
            "feedback",
            "Feedback",
            PARAM_FEEDBACK_ID,
            self.params.feedback(),
            (0.0, 0.6),
            Point {
                x: map_rect.origin.x + (KNOB_W + KNOB_GAP_X) * 2,
                y: row_y,
            },
            &format!("{:.0}%", self.params.feedback() / 0.6 * 100.0),
        );
    }

    fn draw_right_panel(&mut self, ui: &mut Ui<'_>, panel: Rect) {
        let x = panel.origin.x + 16;
        let y = panel.origin.y + PANEL_CONTENT_Y;

        self.draw_param_knob(
            ui,
            "width",
            "Width",
            PARAM_WIDTH_ID,
            self.params.width(),
            (0.0, 1.0),
            Point { x, y },
            &format!("{:.0}%", self.params.width() * 100.0),
        );
        self.draw_param_knob(
            ui,
            "diffusion",
            "Diffusion",
            PARAM_DIFFUSION_ID,
            self.params.diffusion(),
            (0.0, 1.0),
            Point {
                x: x + KNOB_W + KNOB_GAP_X,
                y,
            },
            &format!("{:.0}%", self.params.diffusion() * 100.0),
        );
        self.draw_param_knob(
            ui,
            "air-damping",
            "Air Damping",
            PARAM_AIR_DAMPING_ID,
            self.params.air_damping(),
            (0.0, 1.0),
            Point {
                x,
                y: y + KNOB_W + KNOB_GAP_Y,
            },
            &format!("{:.0}%", self.params.air_damping() * 100.0),
        );

        self.draw_param_toggle(
            ui,
            "air-comp",
            "Air Comp",
            PARAM_AIR_COMP_ID,
            self.params.air_compensation(),
            Point {
                x: x + KNOB_W + KNOB_GAP_X,
                y: y + KNOB_W + KNOB_GAP_Y + 14,
            },
        );

        self.draw_param_toggle(
            ui,
            "clean-dirty",
            "Dirty",
            PARAM_CLEAN_DIRTY_ID,
            self.params.clean_dirty() >= 0.5,
            Point {
                x,
                y: y + (KNOB_W + KNOB_GAP_Y) * 2,
            },
        );

        self.draw_mod_bank(
            ui,
            Rect {
                origin: Point {
                    x,
                    y: y + (KNOB_W + KNOB_GAP_Y) * 2 + 50,
                },
                size: Size {
                    width: (panel.size.width as i32 - 32) as u32,
                    height: (panel.origin.y + panel.size.height as i32
                        - (y + (KNOB_W + KNOB_GAP_Y) * 2 + 60)
                        - 12) as u32,
                },
            },
        );
    }

    fn draw_param_knob(
        &mut self,
        ui: &mut Ui<'_>,
        key: &str,
        name: &str,
        param_id: ClapId,
        mut value: f32,
        range: (f32, f32),
        position: Point,
        value_label: &str,
    ) {
        let prev = ui.cursor();
        ui.set_cursor(position);
        let response = ui.knob_with_key_labels(key, name, value_label, &mut value, range);
        let id = WidgetId::from_label(key);
        self.begin_if_needed(id, param_id, response.active);
        if response.changed {
            self.params.set_param(param_id, value);
            self.push_value(param_id, value);
        }
        ui.set_cursor(prev);
    }

    fn draw_param_toggle(
        &mut self,
        ui: &mut Ui<'_>,
        key: &str,
        label: &str,
        param_id: ClapId,
        mut value: bool,
        position: Point,
    ) {
        let prev = ui.cursor();
        ui.set_cursor(position);
        let response = ui.toggle_with_key(key, label, &mut value, TOGGLE_W, TOGGLE_H);
        if response.changed {
            self.set_param_toggle(param_id, value);
        }
        ui.set_cursor(prev);
    }

    fn draw_pull_button(&mut self, ui: &mut Ui<'_>, rect: Rect) {
        let response = ui.region_with_key("pull-button", rect);
        let active = response.active || self.params.pull_trigger();

        if response.pressed {
            self.push_begin(PARAM_PULL_TRIGGER_ID);
            self.params.set_param(PARAM_PULL_TRIGGER_ID, 1.0);
            self.push_value(PARAM_PULL_TRIGGER_ID, 1.0);
        }
        if response.released {
            self.params.set_param(PARAM_PULL_TRIGGER_ID, 0.0);
            self.push_value(PARAM_PULL_TRIGGER_ID, 0.0);
            self.push_end(PARAM_PULL_TRIGGER_ID);
        }

        let fill = if active {
            ACCENT
        } else if response.hovered {
            Color::rgb(62, 74, 94)
        } else {
            Color::rgb(44, 52, 66)
        };
        ui.canvas().fill_rect(rect, fill);
        ui.canvas().stroke_rect(rect, 1, PANEL_BORDER);
        ui.text_with_color(
            Point {
                x: rect.origin.x + 36,
                y: rect.origin.y + 8,
            },
            "PULL",
            Color::rgb(12, 14, 20),
        );
    }

    fn draw_pull_shape_dropdown(&mut self, ui: &mut Ui<'_>, rect: Rect) {
        let prev = ui.cursor();
        ui.set_cursor(rect.origin);
        let mut selected = self.params.pull_shape_index();
        let response = ui.dropdown_with_key(
            "pull-shape",
            "Pull Shape",
            &PULL_SHAPE_LABELS,
            &mut selected,
            rect.size.width as i32,
            rect.size.height as i32,
        );
        if response.changed {
            let value = pull_shape_value_from_index(selected);
            self.params.set_param(PARAM_PULL_SHAPE_ID, value);
            self.push_begin(PARAM_PULL_SHAPE_ID);
            self.push_value(PARAM_PULL_SHAPE_ID, value);
            self.push_end(PARAM_PULL_SHAPE_ID);
        }
        ui.set_cursor(prev);
    }

    fn draw_mode_cards(&mut self, ui: &mut Ui<'_>, origin: Point, width: i32) {
        for (idx, mode) in ModeCard::ALL.iter().enumerate() {
            let y = origin.y + idx as i32 * (CARD_H + CARD_GAP);
            let rect = Rect {
                origin: Point { x: origin.x, y },
                size: Size {
                    width: width as u32,
                    height: CARD_H as u32,
                },
            };
            let response = ui.region_with_key(mode.key(), rect);
            if response.pressed {
                self.apply_mode(*mode);
            }

            let fill = if self.active_mode == *mode {
                CARD_ACTIVE
            } else if response.hovered {
                Color::rgb(43, 51, 66)
            } else {
                CARD_IDLE
            };
            ui.canvas().fill_rect(rect, fill);
            ui.canvas().stroke_rect(rect, 1, PANEL_BORDER);
            ui.text_with_color(
                Point {
                    x: rect.origin.x + 10,
                    y: rect.origin.y + 9,
                },
                mode.title(),
                TITLE,
            );
            ui.text_with_color(
                Point {
                    x: rect.origin.x + 10,
                    y: rect.origin.y + 26,
                },
                mode.subtitle(),
                SUBTITLE,
            );
        }
    }

    fn apply_mode(&mut self, mode: ModeCard) {
        self.active_mode = mode;
        let updates = mode.updates();
        for (param_id, value) in updates {
            self.push_begin(*param_id);
            self.params.set_param(*param_id, *value);
            self.push_value(*param_id, *value);
            self.push_end(*param_id);
        }
    }

    fn draw_tension_map(&mut self, ui: &mut Ui<'_>, rect: Rect) {
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

        let response = ui.region_with_key("tension-map", rect);
        let input = ui.input();
        if response.pressed {
            self.map_dragging = true;
            self.push_begin(PARAM_PULL_DIRECTION_ID);
            self.push_begin(PARAM_ELASTICITY_ID);
            self.update_map_from_pointer(input.pointer_pos, rect);
        }
        if response.dragged && self.map_dragging {
            self.update_map_from_pointer(input.pointer_pos, rect);
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

        ui.text_with_color(
            Point {
                x: rect.origin.x + 8,
                y: rect.origin.y - 16,
            },
            "Backward",
            SUBTITLE,
        );
        ui.text_with_color(
            Point {
                x: rect.origin.x + rect.size.width as i32 - 58,
                y: rect.origin.y - 16,
            },
            "Forward",
            SUBTITLE,
        );
        ui.text_with_color(
            Point {
                x: rect.origin.x - 2,
                y: rect.origin.y + rect.size.height as i32 + 6,
            },
            "Viscous",
            SUBTITLE,
        );
        ui.text_with_color(
            Point {
                x: rect.origin.x + rect.size.width as i32 - 44,
                y: rect.origin.y + rect.size.height as i32 + 6,
            },
            "Spring",
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

    fn draw_mod_bank(&mut self, ui: &mut Ui<'_>, rect: Rect) {
        let canvas = ui.canvas();
        canvas.fill_rect(rect, Color::rgb(21, 26, 34));
        canvas.stroke_rect(rect, 1, PANEL_BORDER);
        ui.text_with_color(
            Point {
                x: rect.origin.x + 8,
                y: rect.origin.y + 6,
            },
            "Slow Mod Bank",
            TITLE,
        );

        let mut row_y = rect.origin.y + 24;
        let col_x = [rect.origin.x + 8, rect.origin.x + 82, rect.origin.x + 156];

        self.draw_local_toggle(
            ui,
            "mod-run",
            "Run",
            &mut self.mod_bank.run,
            Point {
                x: rect.origin.x + rect.size.width as i32 - 76,
                y: rect.origin.y + 6,
            },
        );

        self.draw_mod_row(
            ui,
            "LFO A",
            &mut self.mod_bank.lfo_a_rate,
            &mut self.mod_bank.lfo_a_depth,
            Some(&mut self.mod_bank.lfo_a_drift),
            row_y,
            col_x,
        );
        row_y += 44;
        self.draw_mod_row(
            ui,
            "LFO B",
            &mut self.mod_bank.lfo_b_rate,
            &mut self.mod_bank.lfo_b_depth,
            Some(&mut self.mod_bank.lfo_b_drift),
            row_y,
            col_x,
        );
        row_y += 44;
        self.draw_mod_row(
            ui,
            "Random",
            &mut self.mod_bank.walk_rate,
            &mut self.mod_bank.walk_depth,
            None,
            row_y,
            col_x,
        );
        row_y += 44;
        self.draw_mod_row(
            ui,
            "Env",
            &mut self.mod_bank.env_sensitivity,
            &mut self.mod_bank.env_depth,
            None,
            row_y,
            col_x,
        );

        let grid_origin = Point {
            x: rect.origin.x + 8,
            y: row_y + 42,
        };
        ui.text_with_color(
            Point {
                x: grid_origin.x,
                y: grid_origin.y - 16,
            },
            "Routes: Ten Dir Grn Wid",
            SUBTITLE,
        );
        for src in 0..4 {
            ui.text_with_color(
                Point {
                    x: grid_origin.x,
                    y: grid_origin.y + src as i32 * 18,
                },
                match src {
                    0 => "A",
                    1 => "B",
                    2 => "R",
                    _ => "E",
                },
                TITLE,
            );
            for dst in 0..4 {
                let key = format!("route-{src}-{dst}");
                self.draw_local_toggle(
                    ui,
                    &key,
                    "",
                    &mut self.mod_bank.routes[src][dst],
                    Point {
                        x: grid_origin.x + 18 + dst as i32 * 26,
                        y: grid_origin.y + src as i32 * 18,
                    },
                );
            }
        }
    }

    fn draw_mod_row(
        &mut self,
        ui: &mut Ui<'_>,
        label: &str,
        rate: &mut f32,
        depth: &mut f32,
        drift: Option<&mut f32>,
        y: i32,
        cols: [i32; 3],
    ) {
        ui.text_with_color(Point { x: cols[0], y }, label, TITLE);
        self.draw_local_knob(
            ui,
            &format!("{label}-rate"),
            "R",
            rate,
            (0.01, 1.5),
            Point {
                x: cols[0] + 18,
                y: y - 10,
            },
        );
        self.draw_local_knob(
            ui,
            &format!("{label}-depth"),
            "D",
            depth,
            (0.0, 1.0),
            Point {
                x: cols[1] + 18,
                y: y - 10,
            },
        );
        if let Some(drift) = drift {
            self.draw_local_knob(
                ui,
                &format!("{label}-drift"),
                "Dr",
                drift,
                (0.0, 1.0),
                Point {
                    x: cols[2] + 18,
                    y: y - 10,
                },
            );
        }
    }

    fn draw_local_knob(
        &mut self,
        ui: &mut Ui<'_>,
        key: &str,
        label: &str,
        value: &mut f32,
        range: (f32, f32),
        position: Point,
    ) {
        let prev = ui.cursor();
        ui.set_cursor(position);
        let value_label = format!("{:.2}", *value);
        let _ = ui.knob_with_key_labels(key, label, &value_label, value, range);
        ui.set_cursor(prev);
    }

    fn draw_local_toggle(
        &mut self,
        ui: &mut Ui<'_>,
        key: &str,
        label: &str,
        value: &mut bool,
        position: Point,
    ) {
        let prev = ui.cursor();
        ui.set_cursor(position);
        let _ = ui.toggle_with_key(key, label, value, 20, 12);
        ui.set_cursor(prev);
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

    fn draw_meters(&mut self, ui: &mut Ui<'_>, panel: Rect) {
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
        let labels = [
            "In L", "In R", "Elastic", "Warp", "Space", "Feed", "Out L", "Out R", "Tension",
        ];

        let inner_x = panel.origin.x + 16;
        let inner_y = panel.origin.y + PANEL_CONTENT_Y;
        let meter_w = ((panel.size.width as i32 - 32) / labels.len() as i32).max(14);
        let max_h = panel.size.height as i32 - PANEL_CONTENT_Y - 18;

        for (index, label) in labels.iter().enumerate() {
            self.meter_smooth[index] += (values[index] - self.meter_smooth[index]) * 0.2;
            let value = self.meter_smooth[index].clamp(0.0, 1.0);
            let x = inner_x + index as i32 * meter_w;
            let bar_rect = Rect {
                origin: Point { x, y: inner_y },
                size: Size {
                    width: (meter_w - 8).max(8) as u32,
                    height: max_h.max(8) as u32,
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
                    x,
                    y: inner_y + max_h + 4,
                },
                label,
                SUBTITLE,
            );
        }
    }
}

fn draw_panel(ui: &mut Ui<'_>, rect: Rect, title: &str) {
    ui.canvas().fill_rect(rect, PANEL_BG);
    ui.canvas().stroke_rect(rect, 1, PANEL_BORDER);
    ui.text_with_color(
        Point {
            x: rect.origin.x + 10,
            y: rect.origin.y + PANEL_TITLE_Y,
        },
        title,
        TITLE,
    );
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ModeCard {
    TapeTugPad,
    ElasticDroneMaker,
    RatchetAtmos,
    GhostPercTail,
}

impl ModeCard {
    const ALL: [ModeCard; 4] = [
        Self::TapeTugPad,
        Self::ElasticDroneMaker,
        Self::RatchetAtmos,
        Self::GhostPercTail,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::TapeTugPad => "mode-tape-tug",
            Self::ElasticDroneMaker => "mode-drone-maker",
            Self::RatchetAtmos => "mode-ratchet-atmos",
            Self::GhostPercTail => "mode-ghost-tail",
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

    fn subtitle(self) -> &'static str {
        match self {
            Self::TapeTugPad => "Smooth wide stretch",
            Self::ElasticDroneMaker => "Held drift feedback",
            Self::RatchetAtmos => "Stepped pull motion",
            Self::GhostPercTail => "Sustain pull focus",
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
            run: false,
            lfo_a_rate: 0.16,
            lfo_a_depth: 0.35,
            lfo_a_drift: 0.2,
            lfo_b_rate: 0.09,
            lfo_b_depth: 0.28,
            lfo_b_drift: 0.25,
            walk_rate: 0.12,
            walk_depth: 0.22,
            env_sensitivity: 0.4,
            env_depth: 0.35,
            routes: [
                [true, false, false, false],
                [false, true, false, false],
                [false, false, true, false],
                [false, false, false, true],
            ],
            phase_a: 0.0,
            phase_b: 0.3,
            walk_state: 0.0,
            env_state: 0.0,
            last_sent: [0.0; 4],
            noise_state: 0x1A2B_3C4D,
        }
    }
}

fn triangle(phase: f32) -> f32 {
    let p = phase.fract();
    if p < 0.5 {
        p * 4.0 - 1.0
    } else {
        3.0 - p * 4.0
    }
}

fn signed_noise(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    ((x as f32 / u32::MAX as f32) * 2.0) - 1.0
}
