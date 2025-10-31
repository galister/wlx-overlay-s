use std::{cell::RefCell, rc::Rc, sync::Arc};

use button::setup_custom_button;
use glam::{vec2, Affine2, Vec2};
use label::setup_custom_label;
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};
use wgui::{
    assets::AssetPath,
    drawing,
    event::{
        Event as WguiEvent, EventCallback, EventListenerID, EventListenerKind,
        InternalStateChangeEvent, MouseButtonIndex, MouseDownEvent, MouseLeaveEvent,
        MouseMotionEvent, MouseUpEvent, MouseWheelEvent,
    },
    layout::{Layout, LayoutParams, WidgetID},
    parser::ParserState,
    renderer_vk::context::Context as WguiContext,
    widget::{label::WidgetLabel, rectangle::WidgetRectangle, EventResult},
};

use crate::{
    backend::input::{Haptics, HoverResult, PointerHit, PointerMode},
    graphics::{CommandBuffers, ExtentExt},
    state::AppState,
    windowing::backend::{ui_transform, FrameMeta, OverlayBackend, ShouldRender},
};

use super::{timer::GuiTimer, timestep::Timestep};

mod button;
mod helper;
mod label;

const MAX_SIZE: u32 = 2048;
const MAX_SIZE_VEC2: Vec2 = vec2(MAX_SIZE as _, MAX_SIZE as _);

const COLOR_ERR: drawing::Color = drawing::Color::new(1., 0., 1., 1.);

pub struct GuiPanel<S> {
    pub layout: Layout,
    pub state: S,
    pub timers: Vec<GuiTimer>,
    pub parser_state: ParserState,
    interaction_transform: Option<Affine2>,
    context: WguiContext,
    timestep: Timestep,
}

pub type OnCustomIdFunc = Box<
    dyn Fn(
        Rc<str>,
        WidgetID,
        &wgui::parser::ParseDocumentParams,
        &mut Layout,
        &mut ParserState,
    ) -> anyhow::Result<()>,
>;

impl<S: 'static> GuiPanel<S> {
    pub fn new_from_template(
        app: &mut AppState,
        path: &str,
        state: S,
        on_custom_id: Option<OnCustomIdFunc>,
    ) -> anyhow::Result<Self> {
        let custom_elems = Rc::new(RefCell::new(vec![]));

        let doc_params = wgui::parser::ParseDocumentParams {
            globals: app.wgui_globals.clone(),
            path: AssetPath::BuiltIn(path),
            extra: wgui::parser::ParseDocumentExtra {
                on_custom_attribs: Some(Box::new({
                    let custom_elems = custom_elems.clone();
                    move |attribs| {
                        custom_elems.borrow_mut().push(attribs.to_owned());
                    }
                })),
                ..Default::default()
            },
        };

        let (mut layout, mut parser_state) =
            wgui::parser::new_layout_from_assets(&doc_params, &LayoutParams::default())?;

        if let Some(on_element_id) = on_custom_id {
            let ids = parser_state.data.ids.clone(); // FIXME: copying all ids?

            for (id, widget) in ids {
                on_element_id(
                    id.clone(),
                    widget,
                    &doc_params,
                    &mut layout,
                    &mut parser_state,
                )?;
            }
        }

        for elem in custom_elems.borrow().iter() {
            if layout
                .state
                .widgets
                .get_as::<WidgetLabel>(elem.widget_id)
                .is_some()
            {
                setup_custom_label::<S>(&mut layout, elem, app);
            } else if layout
                .state
                .widgets
                .get_as::<WidgetRectangle>(elem.widget_id)
                .is_some()
            {
                setup_custom_button::<S>(&mut layout, elem, app);
            }
        }

        let context = WguiContext::new(&mut app.wgui_shared, 1.0)?;
        let mut timestep = Timestep::new();
        timestep.set_tps(60.0);

        Ok(Self {
            layout,
            context,
            timestep,
            state,
            parser_state,
            timers: vec![],
            interaction_transform: None,
        })
    }

    pub fn new_blank(app: &mut AppState, state: S) -> anyhow::Result<Self> {
        let layout = Layout::new(app.wgui_globals.clone(), &LayoutParams::default())?;
        let context = WguiContext::new(&mut app.wgui_shared, 1.0)?;
        let mut timestep = Timestep::new();
        timestep.set_tps(60.0);

        Ok(Self {
            layout,
            context,
            timestep,
            state,
            parser_state: ParserState::default(),
            timers: vec![],
            interaction_transform: None,
        })
    }

    pub fn update_layout(&mut self) -> anyhow::Result<()> {
        self.layout.update(MAX_SIZE_VEC2, 0.0)
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
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        if self.layout.content_size.x * self.layout.content_size.y != 0.0 {
            self.update_layout()?;
            self.interaction_transform = Some(ui_transform([
                //TODO: dynamic
                self.layout.content_size.x as _,
                self.layout.content_size.y as _,
            ]));
        }
        Ok(())
    }

    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
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
            return Ok(ShouldRender::Unable);
        }

        Ok(if self.layout.check_toggle_needs_redraw() {
            ShouldRender::Should
        } else {
            ShouldRender::Can
        })
    }

    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        self.context
            .update_viewport(&mut app.wgui_shared, tgt.extent_u32arr(), 1.0)?;
        self.layout.update(MAX_SIZE_VEC2, self.timestep.alpha)?;

        let mut cmd_buf = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .unwrap(); // want panic

        cmd_buf.begin_rendering(
            tgt,
            wgui::gfx::cmd::WGfxClearMode::Clear([0.0, 0.0, 0.0, 0.0]),
        )?;

        let primitives = wgui::drawing::draw(&mut wgui::drawing::DrawParams {
            layout: &mut self.layout,
            debug_draw: false,
            alpha,
        })?;
        self.context
            .draw(&mut app.wgui_shared, &mut cmd_buf, &primitives)?;
        cmd_buf.end_rendering()?;
        buf.push(cmd_buf.build()?);

        Ok(true)
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            extent: [
                MAX_SIZE.min(self.layout.content_size.x as _),
                MAX_SIZE.min(self.layout.content_size.y as _),
                1,
            ],
            ..Default::default()
        })
    }

    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta_y: f32, delta_x: f32) {
        let e = WguiEvent::MouseWheel(MouseWheelEvent {
            shift: vec2(delta_x, delta_y),
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
        log::info!("panel: on left");
        let e = WguiEvent::MouseLeave(MouseLeaveEvent { device: pointer });
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
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
}
