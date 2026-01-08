use std::{any, cell::RefCell, rc::Rc};

use button::setup_custom_button;
use glam::{Affine2, Vec2, vec2};
use label::setup_custom_label;
use wgui::{
    assets::AssetPath,
    components::button::ComponentButton,
    drawing,
    event::{
        Event as WguiEvent, EventCallback, EventListenerID, EventListenerKind,
        InternalStateChangeEvent, MouseButtonIndex, MouseDownEvent, MouseLeaveEvent,
        MouseMotionEvent, MouseUpEvent, MouseWheelEvent,
    },
    gfx::cmd::WGfxClearMode,
    layout::{Layout, LayoutParams, LayoutUpdateParams, WidgetID},
    parser::{self, CustomAttribsInfoOwned, Fetchable, ParseDocumentExtra, ParserState},
    renderer_vk::context::Context as WguiContext,
    widget::{EventResult, label::WidgetLabel},
    windowing::context_menu::{self, ContextMenu},
};
use wlx_common::overlays::{BackendAttrib, BackendAttribValue};
use wlx_common::timestep::Timestep;

use crate::{
    app_misc,
    backend::input::{Haptics, HoverResult, PointerHit, PointerMode},
    state::AppState,
    subsystem::hid::WheelDelta,
    windowing::backend::{
        FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender, ui_transform,
    },
};

use super::timer::GuiTimer;

pub mod button;
pub mod device_list;
mod label;
pub mod overlay_list;
pub mod set_list;

const DEFAULT_MAX_SIZE: f32 = 2048.0;

const COLOR_ERR: drawing::Color = drawing::Color::new(1., 0., 1., 1.);

pub type OnNotifyFunc<S> =
    Box<dyn Fn(&mut GuiPanel<S>, &mut AppState, OverlayEventData) -> anyhow::Result<()>>;

pub struct GuiPanel<S> {
    pub layout: Layout,
    pub state: S,
    pub timers: Vec<GuiTimer>,
    pub parser_state: ParserState,
    pub max_size: Vec2,
    pub gui_scale: f32,
    pub on_notify: Option<OnNotifyFunc<S>>,
    pub initialized: bool,
    pub doc_extra: Option<ParseDocumentExtra>,
    interaction_transform: Option<Affine2>,
    context: WguiContext,
    timestep: Timestep,
    has_focus: [bool; 2],
    last_content_size: Vec2,
    custom_elems: Rc<RefCell<Vec<CustomAttribsInfoOwned>>>,
    context_menu: Rc<RefCell<ContextMenu>>,
    on_custom_attrib: Option<OnCustomAttribFunc>,
    on_custom_attrib_inner: parser::OnCustomAttribsFunc,
}

pub type OnCustomIdFunc<S> = Box<
    dyn Fn(
        Rc<str>,
        WidgetID,
        &wgui::parser::ParseDocumentParams,
        &mut Layout,
        &mut ParserState,
        &mut S,
    ) -> anyhow::Result<()>,
>;

pub type OnCustomAttribFunc =
    Box<dyn Fn(&mut Layout, &ParserState, &CustomAttribsInfoOwned, &AppState)>;

pub struct NewGuiPanelParams<S> {
    pub on_custom_id: Option<OnCustomIdFunc<S>>, // used only in `new_from_template`
    pub on_custom_attrib: Option<OnCustomAttribFunc>, // used only in `new_from_template`
    pub resize_to_parent: bool,
    pub external_xml: bool,
    pub gui_scale: f32,
}

impl<S> Default for NewGuiPanelParams<S> {
    fn default() -> Self {
        Self {
            on_custom_id: None,
            on_custom_attrib: None,
            resize_to_parent: false,
            external_xml: false,
            gui_scale: 1.0,
        }
    }
}

impl<S: 'static> GuiPanel<S> {
    pub fn new_from_template(
        app: &mut AppState,
        path: &str,
        mut state: S,
        params: NewGuiPanelParams<S>,
    ) -> anyhow::Result<Self> {
        let custom_elems = Rc::new(RefCell::new(vec![]));

        let on_custom_attrib_inner: parser::OnCustomAttribsFunc = Rc::new({
            let custom_elems = custom_elems.clone();
            move |attribs| {
                custom_elems.borrow_mut().push(attribs.to_owned());
            }
        });

        let doc_params = wgui::parser::ParseDocumentParams {
            globals: app.wgui_globals.clone(),
            path: if params.external_xml {
                AssetPath::File(path)
            } else {
                AssetPath::FileOrBuiltIn(path)
            },
            extra: wgui::parser::ParseDocumentExtra {
                on_custom_attribs: Some(on_custom_attrib_inner.clone()),
                ..Default::default()
            },
        };

        let (mut layout, mut parser_state) = wgui::parser::new_layout_from_assets(
            &doc_params,
            &LayoutParams {
                resize_to_parent: params.resize_to_parent,
            },
        )?;

        if let Some(on_element_id) = params.on_custom_id {
            let ids = parser_state.data.ids.clone(); // FIXME: copying all ids?

            for (id, widget) in ids {
                on_element_id(
                    id.clone(),
                    widget,
                    &doc_params,
                    &mut layout,
                    &mut parser_state,
                    &mut state,
                )?;
            }
        }

        let context = WguiContext::new(&mut app.wgui_shared, 1.0)?;
        let timestep = Timestep::new(60.0);

        let mut me = Self {
            layout,
            context,
            timestep,
            state,
            parser_state,
            max_size: vec2(DEFAULT_MAX_SIZE as _, DEFAULT_MAX_SIZE as _),
            timers: vec![],
            interaction_transform: None,
            on_notify: None,
            gui_scale: params.gui_scale,
            initialized: false,
            has_focus: [false, false],
            last_content_size: Vec2::ZERO,
            doc_extra: Some(doc_params.extra),
            custom_elems,
            context_menu: Default::default(),
            on_custom_attrib: params.on_custom_attrib,
            on_custom_attrib_inner,
        };
        me.process_custom_elems(app);

        Ok(me)
    }

    /// Perform initial setup on newly added elements.
    pub fn process_custom_elems(&mut self, app: &mut AppState) {
        let mut elems = self.custom_elems.borrow_mut();
        for elem in elems.iter() {
            if self
                .layout
                .state
                .widgets
                .get_as::<WidgetLabel>(elem.widget_id)
                .is_some()
            {
                setup_custom_label::<S>(&mut self.layout, elem, app);
            } else if let Ok(button) = self
                .parser_state
                .fetch_component_from_widget_id_as::<ComponentButton>(elem.widget_id)
            {
                setup_custom_button::<S>(
                    &mut self.layout,
                    &self.parser_state,
                    elem,
                    &self.context_menu,
                    &self.on_custom_attrib_inner,
                    button,
                );
            }

            if let Some(on_custom_attrib) = &self.on_custom_attrib {
                on_custom_attrib(&mut self.layout, &self.parser_state, elem, app);
            }
        }
        elems.clear();
    }

    pub fn update_layout(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        app_misc::process_layout_result(
            app,
            self.layout.update(&mut LayoutUpdateParams {
                size: self.max_size / self.gui_scale,
                timestep_alpha: self.timestep.alpha,
            })?,
        );
        Ok(())
    }

    pub fn push_event(&mut self, app: &mut AppState, event: &WguiEvent) -> EventResult {
        match self.layout.push_event(event, app, &mut self.state) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to push event: {e:?}");
                EventResult::NoHit
            }
        }
    }

    pub fn add_event_listener(
        &mut self,
        widget_id: WidgetID,
        kind: EventListenerKind,
        callback: EventCallback<AppState, S>,
    ) -> Option<EventListenerID> {
        self.layout.add_event_listener(widget_id, kind, callback)
    }
}

impl<S: 'static> OverlayBackend for GuiPanel<S> {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if self.layout.content_size.x * self.layout.content_size.y != 0.0 {
            self.update_layout(app)?;
            self.interaction_transform = Some(ui_transform([
                self.layout.content_size.x as _,
                self.layout.content_size.y as _,
            ]));
            self.initialized = true;
        }
        Ok(())
    }

    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.layout.needs_redraw = true;
        self.timestep.reset();
        Ok(())
    }

    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        //TODO: this only executes one timer event per frame
        if let Some(signal) = self.timers.iter_mut().find_map(GuiTimer::check_tick) {
            self.push_event(
                app,
                &WguiEvent::InternalStateChange(InternalStateChangeEvent { metadata: signal }),
            );
        }

        while self.timestep.on_tick() {
            self.layout.tick()?;
        }

        if self.layout.content_size.x * self.layout.content_size.y == 0.0 {
            log::trace!("Unable to render: content size 0");
            return Ok(ShouldRender::Unable);
        }

        let tick_result = self
            .context_menu
            .borrow_mut()
            .tick(&mut self.layout, &mut self.parser_state)?;
        if matches!(tick_result, context_menu::TickResult::Opened) {
            self.process_custom_elems(app);
        }

        if !self
            .last_content_size
            .abs_diff_eq(self.layout.content_size, 0.1 /* pixels */)
        {
            self.interaction_transform = Some(ui_transform([
                self.layout.content_size.x as _,
                self.layout.content_size.y as _,
            ]));
            self.last_content_size = self.layout.content_size;
        }

        Ok(if self.layout.check_toggle_needs_redraw() {
            ShouldRender::Should
        } else {
            ShouldRender::Can
        })
    }

    fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()> {
        self.context
            .update_viewport(&mut app.wgui_shared, rdr.extent, self.gui_scale)?;
        self.update_layout(app)?;
        let globals = self.layout.state.globals.clone(); // sorry
        let mut globals = globals.get();

        let primitives = wgui::drawing::draw(&mut wgui::drawing::DrawParams {
            globals: &mut globals,
            layout: &mut self.layout,
            debug_draw: false,
            timestep_alpha: self.timestep.alpha,
        })?;
        self.context.draw(
            &globals.font_system,
            &mut app.wgui_shared,
            rdr.cmd_buf_single(),
            &primitives,
        )?;
        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            clear: WGfxClearMode::Clear([0., 0., 0., 0.]),
            extent: [
                self.max_size.x.min(self.layout.content_size.x) as _,
                self.max_size.y.min(self.layout.content_size.y) as _,
                1,
            ],
            ..Default::default()
        })
    }

    fn notify(&mut self, app: &mut AppState, data: OverlayEventData) -> anyhow::Result<()> {
        let Some(on_notify) = self.on_notify.take() else {
            return Ok(());
        };
        on_notify(self, app, data)?;
        self.on_notify = Some(on_notify);
        Ok(())
    }

    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: WheelDelta) {
        let e = WguiEvent::MouseWheel(MouseWheelEvent {
            delta: vec2(delta.x, delta.y),
            pos: hit.uv * self.layout.content_size,
            device: hit.pointer,
        });
        self.push_event(app, &e);
    }

    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> HoverResult {
        let e = &WguiEvent::MouseMotion(MouseMotionEvent {
            pos: hit.uv * self.layout.content_size,
            device: hit.pointer,
        });

        self.has_focus[hit.pointer] = true;

        let result = self.push_event(app, e);

        HoverResult {
            consume: result != EventResult::NoHit,
            haptics: self
                .layout
                .check_toggle_haptics_triggered()
                .then_some(Haptics {
                    intensity: 0.1,
                    duration: 0.01,
                    frequency: 5.0,
                }),
        }
    }

    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        let e = WguiEvent::MouseLeave(MouseLeaveEvent { device: pointer });
        self.has_focus[pointer] = false;
        self.push_event(app, &e);
    }

    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let index = match hit.mode {
            PointerMode::Left => MouseButtonIndex::Left,
            PointerMode::Right => MouseButtonIndex::Right,
            PointerMode::Middle => MouseButtonIndex::Middle,
            _ => return,
        };

        let e = if pressed {
            WguiEvent::MouseDown(MouseDownEvent {
                pos: hit.uv * self.layout.content_size,
                index,
                device: hit.pointer,
            })
        } else {
            WguiEvent::MouseUp(MouseUpEvent {
                pos: hit.uv * self.layout.content_size,
                index,
                device: hit.pointer,
            })
        };
        self.push_event(app, &e);

        // released while off-panel → send mouse leave as well
        if !pressed && !self.has_focus[hit.pointer] {
            let e = WguiEvent::MouseMotion(MouseMotionEvent {
                pos: vec2(-1., -1.),
                device: hit.pointer,
            });
            self.push_event(app, &e);
            let e = WguiEvent::MouseLeave(MouseLeaveEvent {
                device: hit.pointer,
            });
            self.push_event(app, &e);
        }
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
    fn get_attrib(&self, _attrib: BackendAttrib) -> Option<BackendAttribValue> {
        None
    }
    fn set_attrib(&mut self, _app: &mut AppState, _value: BackendAttribValue) -> bool {
        false
    }
}
