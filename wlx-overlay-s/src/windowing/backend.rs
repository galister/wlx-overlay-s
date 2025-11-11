use glam::{Affine2, Affine3A, Vec2};
use std::{any::Any, sync::Arc};
use vulkano::{format::Format, image::view::ImageView};

use crate::{
    backend::input::{HoverResult, PointerHit},
    graphics::CommandBuffers,
    state::AppState,
    subsystem::hid::WheelDelta,
};

#[derive(Default, Clone, Copy)]
pub struct FrameMeta {
    pub extent: [u32; 3],
    pub transform: Affine3A,
    pub format: Format,
}

pub enum ShouldRender {
    /// The overlay is dirty and needs to be rendered.
    Should,
    /// The overlay is not dirty but is ready to be rendered.
    Can,
    /// The overlay is not ready to be rendered.
    Unable,
}

pub trait OverlayBackend: Any {
    /// Called once, before the first frame is rendered
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()>;

    /// Called when the presentation layer is ready to present a new frame
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender>;

    /// Called when the contents need to be rendered to the swapchain
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool>;

    /// Called to retrieve the effective extent of the image
    /// Used for creating swapchains.
    ///
    /// Must be Some if should_render was Should or Can on the same frame.
    fn frame_meta(&mut self) -> Option<FrameMeta>;

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
    fn render(
        &mut self,
        _: &mut AppState,
        _: Arc<ImageView>,
        _: &mut CommandBuffers,
        _: f32,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        None
    }

    fn on_hover(&mut self, _: &mut AppState, _: &PointerHit) -> HoverResult {
        HoverResult::default()
    }
    fn on_left(&mut self, _: &mut AppState, _: usize) {}
    fn on_pointer(&mut self, _: &mut AppState, _: &PointerHit, _: bool) {}
    fn on_scroll(&mut self, _: &mut AppState, _: &PointerHit, _: f32, _: f32) {}
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        None
    }
}
