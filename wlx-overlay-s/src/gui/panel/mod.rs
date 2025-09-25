use std::{cell::RefCell, rc::Rc, sync::Arc};

use button::setup_custom_button;
use glam::{Affine2, Vec2, vec2};
use label::setup_custom_label;
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};
use wgui::{
    drawing,
    event::{
        Event as WguiEvent, EventListenerCollection, InternalStateChangeEvent, ListenerHandleVec,
        MouseButtonIndex, MouseDownEvent, MouseLeaveEvent, MouseMotionEvent, MouseUpEvent,
        MouseWheelEvent,
    },
    layout::{Layout, LayoutParams, WidgetID},
    parser::ParserState,
    renderer_vk::context::Context as WguiContext,
    widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

use crate::{
    backend::{
        input::{Haptics, PointerHit, PointerMode},
        overlay::{FrameMeta, OverlayBackend, ShouldRender, ui_transform},
    },
    graphics::{CommandBuffers, ExtentExt},
    state::AppState,
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
    pub listeners: EventListenerCollection<AppState, S>,
    pub listener_handles: ListenerHandleVec,
    pub parser_state: ParserState,
    interaction_transform: Option<Affine2>,
    context: WguiContext,
    timestep: Timestep,
}

pub type OnCustomIdFunc<S> = Box<
    dyn Fn(
        Rc<str>,
        WidgetID,
        &wgui::parser::ParseDocumentParams,
        &mut Layout,
        &mut ParserState,
        &mut EventListenerCollection<AppState, S>,
    ) -> anyhow::Result<()>,
>;

impl<S> GuiPanel<S> {
    pub fn new_from_template(
        app: &mut AppState,
        path: &str,
        state: S,
        on_custom_id: Option<OnCustomIdFunc<S>>,
    ) -> anyhow::Result<Self> {
        let mut listeners = EventListenerCollection::<AppState, S>::default();
        let mut listener_handles = ListenerHandleVec::default();

        let custom_elems = Rc::new(RefCell::new(vec![]));

        let doc_params = wgui::parser::ParseDocumentParams {
            globals: app.wgui_globals.clone(),
            path,
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

        let (mut layout, mut parser_state) = wgui::parser::new_layout_from_assets(
            &mut listeners,
            &doc_params,
            &LayoutParams::default(),
        )?;

        if let Some(on_element_id) = on_custom_id {
            let ids = parser_state.data.ids.clone(); // FIXME: copying all ids?

            for (id, widget) in ids {
                on_element_id(
                    id.clone(),
                    widget,
                    &doc_params,
                    &mut layout,
                    &mut parser_state,
                    &mut listeners,
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
                setup_custom_label(
                    &mut layout,
                    elem,
                    &mut listeners,
                    &mut listener_handles,
                    app,
                );
            } else if layout
                .state
                .widgets
                .get_as::<WidgetRectangle>(elem.widget_id)
                .is_some()
            {
                setup_custom_button(elem, &mut listeners, &mut listener_handles, app);
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
            listener_handles,
            parser_state,
            timers: vec![],
            listeners,
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
            listener_handles: ListenerHandleVec::default(),
            timers: vec![],
            listeners: EventListenerCollection::default(),
            interaction_transform: None,
        })
    }

    pub fn update_layout(&mut self) -> anyhow::Result<()> {
        self.layout.update(MAX_SIZE_VEC2, 0.0)
    }

    pub fn push_event(&mut self, app: &mut AppState, event: &WguiEvent) {
        if let Err(e) = self
            .layout
            .push_event(&mut self.listeners, event, (app, &mut self.state))
        {
            log::error!("Failed to push event: {e:?}");
        }
    }
}

impl<S> OverlayBackend for GuiPanel<S> {
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
        _alpha: f32,
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

        let primitives = wgui::drawing::draw(&wgui::drawing::DrawParams {
            layout: &self.layout,
            debug_draw: false,
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
        self.layout
            .push_event(
                &mut self.listeners,
                &WguiEvent::MouseWheel(MouseWheelEvent {
                    shift: vec2(delta_x, delta_y),
                    pos: hit.uv * self.layout.content_size,
                    device: hit.pointer,
                }),
                (app, &mut self.state),
            )
            .unwrap(); // want panic
    }

    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        self.push_event(
            app,
            &WguiEvent::MouseMotion(MouseMotionEvent {
                pos: hit.uv * self.layout.content_size,
                device: hit.pointer,
            }),
        );

        self.layout
            .check_toggle_haptics_triggered()
            .then_some(Haptics {
                intensity: 0.1,
                duration: 0.01,
                frequency: 5.0,
            })
    }

    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.push_event(
            app,
            &WguiEvent::MouseLeave(MouseLeaveEvent { device: pointer }),
        );
    }

    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let index = match hit.mode {
            PointerMode::Left => MouseButtonIndex::Left,
            PointerMode::Right => MouseButtonIndex::Right,
            PointerMode::Middle => MouseButtonIndex::Middle,
            _ => return,
        };

        if pressed {
            self.push_event(
                app,
                &WguiEvent::MouseDown(MouseDownEvent {
                    pos: hit.uv * self.layout.content_size,
                    index,
                    device: hit.pointer,
                }),
            );
        } else {
            self.push_event(
                app,
                &WguiEvent::MouseUp(MouseUpEvent {
                    pos: hit.uv * self.layout.content_size,
                    index,
                    device: hit.pointer,
                }),
            );
        }
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
}
