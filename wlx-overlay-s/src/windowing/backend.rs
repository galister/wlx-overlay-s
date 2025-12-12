use glam::{Affine2, Affine3A, Vec2};
use std::{any::Any, sync::Arc};
use vulkano::{
    command_buffer::{CommandBufferUsage, PrimaryAutoCommandBuffer},
    device::Queue,
    format::Format,
    image::view::ImageView,
};
use wgui::gfx::{
    WGfx,
    cmd::{GfxCommandBuffer, WGfxClearMode},
};

use crate::{
    backend::input::{HoverResult, PointerHit},
    graphics::ExtentExt,
    state::AppState,
    subsystem::hid::WheelDelta,
    windowing::{OverlayID, window::OverlayCategory},
};

#[derive(Default, Clone, Copy)]
pub struct FrameMeta {
    pub extent: [u32; 3],
    pub transform: Affine3A,
    pub format: Format,
    pub clear: WGfxClearMode,
}

pub enum ShouldRender {
    /// The overlay is dirty and needs to be rendered.
    Should,
    /// The overlay is not dirty but is ready to be rendered.
    Can,
    /// The overlay is not ready to be rendered.
    Unable,
}

pub struct RenderResources {
    pub alpha: f32,
    pub cmd_buf: GfxCommandBuffer,
    pub extent: [u32; 2],
}

impl RenderResources {
    pub fn new(
        gfx: Arc<WGfx>,
        tgt: Arc<ImageView>,
        meta: &FrameMeta,
        alpha: f32,
    ) -> anyhow::Result<Self> {
        let mut cmd_buf = gfx.create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buf.begin_rendering(tgt, meta.clear)?;

        Ok(Self {
            cmd_buf,
            alpha,
            extent: meta.extent.extent_u32arr(),
        })
    }

    pub fn end(mut self) -> anyhow::Result<(Arc<Queue>, Arc<PrimaryAutoCommandBuffer>)> {
        self.cmd_buf.end_rendering()?;
        Ok((self.cmd_buf.queue.clone(), self.cmd_buf.build()?))
    }
}

pub struct OverlayMeta {
    pub id: OverlayID,
    pub name: Arc<str>,
    pub category: OverlayCategory,
}

#[allow(clippy::enum_variant_names)]
pub enum OverlayEventData {
    ActiveSetChanged(Option<usize>),
    NumSetsChanged(usize),
    EditModeChanged(bool),
    OverlaysChanged(Vec<OverlayMeta>),
    DevicesChanged,
}

pub trait OverlayBackend: Any {
    /// Called once, before the first frame is rendered
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()>;

    /// Called when the presentation layer is ready to present a new frame
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender>;

    /// Called when the contents need to be rendered to the swapchain
    fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()>;

    /// Called to retrieve the effective extent of the image
    /// Used for creating swapchains.
    ///
    /// Must be Some if should_render was Should or Can on the same frame.
    fn frame_meta(&mut self) -> Option<FrameMeta>;

    fn notify(&mut self, app: &mut AppState, event_data: OverlayEventData) -> anyhow::Result<()>;

    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> HoverResult;
    fn on_left(&mut self, app: &mut AppState, pointer: usize);
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool);
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: WheelDelta);
    fn get_interaction_transform(&mut self) -> Option<Affine2>;
}

pub fn ui_transform(extent: [u32; 2]) -> Affine2 {
    let aspect = extent[0] as f32 / extent[1] as f32;
    let scale = if aspect < 1.0 {
        Vec2 {
            x: 1.0 / aspect,
            y: -1.0,
        }
    } else {
        Vec2 { x: 1.0, y: -aspect }
    };
    let center = Vec2 { x: 0.5, y: 0.5 };
    Affine2::from_scale_angle_translation(scale, 0.0, center)
}

pub struct DummyBackend {}

impl OverlayBackend for DummyBackend {
    fn init(&mut self, _: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn pause(&mut self, _: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn resume(&mut self, _: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, _: &mut AppState) -> anyhow::Result<ShouldRender> {
        Ok(ShouldRender::Unable)
    }
    fn render(&mut self, _: &mut AppState, _: &mut RenderResources) -> anyhow::Result<()> {
        unreachable!()
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        unreachable!()
    }

    fn notify(&mut self, _: &mut AppState, _event_data: OverlayEventData) -> anyhow::Result<()> {
        unreachable!()
    }

    fn on_hover(&mut self, _: &mut AppState, _: &PointerHit) -> HoverResult {
        HoverResult::default()
    }
    fn on_left(&mut self, _: &mut AppState, _: usize) {}
    fn on_pointer(&mut self, _: &mut AppState, _: &PointerHit, _: bool) {}
    fn on_scroll(&mut self, _: &mut AppState, _: &PointerHit, _: WheelDelta) {}
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        None
    }
}
