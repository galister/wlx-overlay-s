use std::sync::Arc;

use glam::vec2;
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};
use wgui::{
    event::{Event as WguiEvent, MouseDownEvent, MouseMotionEvent, MouseUpEvent, MouseWheelEvent},
    layout::Layout,
    renderer_vk::context::Context as WguiContext,
};

use crate::{
    backend::{
        input::{Haptics, InteractionHandler, PointerHit},
        overlay::{FrameMeta, OverlayBackend, OverlayRenderer, ShouldRender},
    },
    graphics::{CommandBuffers, ExtentExt},
    state::AppState,
};

use super::{asset::GuiAsset, timestep::Timestep};

pub struct GuiPanel {
    pub layout: Layout,
    context: WguiContext,
    timestep: Timestep,
    pub width: u32,
    pub height: u32,
}

impl GuiPanel {
    pub fn new_from_template(
        app: &AppState,
        width: u32,
        height: u32,
        path: &str,
    ) -> anyhow::Result<Self> {
        let mut me = Self::new_blank(app, width, height)?;

        let parent = me.layout.root_widget;
        let _res = wgui::parser::parse_from_assets(&mut me.layout, parent, path)?;

        Ok(me)
    }

    pub fn new_blank(app: &AppState, width: u32, height: u32) -> anyhow::Result<Self> {
        let layout = Layout::new(Box::new(GuiAsset {}))?;
        let context = WguiContext::new(app.gfx.clone(), app.gfx.surface_format, 1.0)?;
        let mut timestep = Timestep::new();
        timestep.set_tps(60.0);

        Ok(Self {
            layout,
            context,
            timestep,
            width,
            height,
        })
    }
}

impl OverlayBackend for GuiPanel {
    fn set_renderer(&mut self, _: Box<dyn OverlayRenderer>) {
        log::debug!("Attempted to replace renderer on GuiPanel!");
    }
    fn set_interaction(&mut self, _: Box<dyn InteractionHandler>) {
        log::debug!("Attempted to replace interaction layer on GuiPanel!");
    }
}

impl InteractionHandler for GuiPanel {
    fn on_scroll(&mut self, _app: &mut AppState, hit: &PointerHit, delta_y: f32, delta_x: f32) {
        self.layout
            .push_event(&WguiEvent::MouseWheel(MouseWheelEvent {
                shift: vec2(delta_x, delta_y),
                pos: hit.uv,
            }))
            .unwrap()
    }

    fn on_hover(&mut self, _app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        self.layout
            .push_event(&WguiEvent::MouseMotion(MouseMotionEvent { pos: hit.uv }))
            .unwrap();

        None
    }

    fn on_left(&mut self, _app: &mut AppState, _pointer: usize) {
        //TODO: is this needed?
    }

    fn on_pointer(&mut self, _app: &mut AppState, hit: &PointerHit, pressed: bool) {
        if pressed {
            self.layout
                .push_event(&WguiEvent::MouseDown(MouseDownEvent { pos: hit.uv }))
                .unwrap();
        } else {
            self.layout
                .push_event(&WguiEvent::MouseUp(MouseUpEvent { pos: hit.uv }))
                .unwrap();
        }
    }
}

impl OverlayRenderer for GuiPanel {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.timestep.reset();
        Ok(())
    }

    fn should_render(&mut self, _app: &mut AppState) -> anyhow::Result<ShouldRender> {
        while self.timestep.on_tick() {
            self.layout.tick()?;
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
        self.context.update_viewport(tgt.extent_u32arr(), 1.0)?;
        self.layout.update(tgt.extent_vec2(), self.timestep.alpha)?;

        let mut cmd_buf = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .unwrap();

        cmd_buf.begin_rendering(tgt)?;
        let primitives = wgui::drawing::draw(&self.layout)?;
        self.context.draw(&app.gfx, &mut cmd_buf, &primitives)?;
        cmd_buf.end_rendering()?;
        buf.push(cmd_buf.build()?);

        Ok(true)
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            extent: [self.width, self.height, 1],
            ..Default::default()
        })
    }
}
