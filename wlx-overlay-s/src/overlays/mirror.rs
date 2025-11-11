use std::{
    sync::Arc,
    task::{Context, Poll},
};

use futures::{Future, FutureExt};
use glam::{Affine2, Affine3A, Vec3};
use vulkano::image::view::ImageView;
use wlx_capture::pipewire::{PipewireCapture, PipewireSelectScreenResult, pipewire_select_screen};

use crate::{
    backend::{
        input::{HoverResult, PointerHit},
        task::TaskType,
    },
    graphics::CommandBuffers,
    state::{AppSession, AppState},
    subsystem::hid::WheelDelta,
    windowing::{
        OverlaySelector,
        backend::{FrameMeta, OverlayBackend, ShouldRender, ui_transform},
        window::{OverlayWindowConfig, OverlayWindowState},
    },
};

use super::screen::backend::ScreenBackend;
type PinnedSelectorFuture = core::pin::Pin<
    Box<dyn Future<Output = Result<PipewireSelectScreenResult, wlx_capture::pipewire::AshpdError>>>,
>;

pub struct MirrorBackend {
    name: Arc<str>,
    renderer: Option<ScreenBackend>,
    selector: Option<PinnedSelectorFuture>,
    last_extent: [u32; 3],
    interaction_transform: Option<Affine2>,
}
impl MirrorBackend {
    pub fn new(name: Arc<str>) -> Self {
        let selector = Box::pin(pipewire_select_screen(None, false, false, false, false));
        Self {
            name,
            renderer: None,
            selector: Some(selector),
            last_extent: [0; 3],
            interaction_transform: None,
        }
    }
}

impl OverlayBackend for MirrorBackend {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        if let Some(mut selector) = self.selector.take() {
            let maybe_pw_result = match selector
                .poll_unpin(&mut Context::from_waker(futures::task::noop_waker_ref()))
            {
                Poll::Ready(result) => result,
                Poll::Pending => {
                    self.selector = Some(selector);
                    return Ok(ShouldRender::Unable);
                }
            };

            match maybe_pw_result {
                Ok(pw_result) => {
                    let node_id = pw_result.streams.first().unwrap().node_id; // streams guaranteed to have at least one element
                    log::info!("{}: PipeWire node selected: {}", self.name.clone(), node_id);
                    let capture = PipewireCapture::new(self.name.clone(), node_id);
                    self.renderer =
                        Some(ScreenBackend::new_raw(self.name.clone(), Box::new(capture)));
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Name(self.name.clone()),
                        Box::new(|app, o| {
                            o.activate(app);
                        }),
                    ));
                }
                Err(e) => {
                    log::warn!("Failed to create mirror due to PipeWire error: {e:?}");
                    self.renderer = None;
                    // drop self
                    app.tasks
                        .enqueue(TaskType::DropOverlay(OverlaySelector::Name(
                            self.name.clone(),
                        )));
                }
            }
        }
        self.renderer
            .as_mut()
            .map_or(Ok(ShouldRender::Unable), |r| r.should_render(app))
    }
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        let mut result = false;
        if let Some(renderer) = self.renderer.as_mut() {
            result = renderer.render(app, tgt, buf, alpha)?;
            if let Some(meta) = renderer.frame_meta() {
                let extent = meta.extent;
                if self.last_extent != extent {
                    self.last_extent = extent;
                    self.interaction_transform = Some(ui_transform([extent[0], extent[1]]));
                }
            }
        }

        Ok(result)
    }
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.pause(app)?;
        }
        Ok(())
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.resume(app)?;
        }
        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.renderer.as_mut().and_then(ScreenBackend::frame_meta)
    }

    fn on_hover(&mut self, _: &mut AppState, _: &PointerHit) -> HoverResult {
        HoverResult {
            consume: true,
            ..HoverResult::default()
        }
    }
    fn on_left(&mut self, _: &mut AppState, _: usize) {}
    fn on_pointer(&mut self, _: &mut AppState, _: &PointerHit, _: bool) {}
    fn on_scroll(&mut self, _: &mut AppState, _: &PointerHit, _delta: WheelDelta) {}
    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
}

pub fn new_mirror(name: Arc<str>, session: &AppSession) -> OverlayWindowConfig {
    OverlayWindowConfig {
        name: name.clone(),
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            transform: Affine3A::from_scale(Vec3::ONE * 0.5 * session.config.desktop_view_scale),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(MirrorBackend::new(name)))
    }
}
