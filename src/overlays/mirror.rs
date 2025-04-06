use std::{
    sync::Arc,
    task::{Context, Poll},
};

use futures::{Future, FutureExt};
use vulkano::image::view::ImageView;
use wlx_capture::pipewire::{pipewire_select_screen, PipewireCapture, PipewireSelectScreenResult};

use crate::{
    backend::{
        common::OverlaySelector,
        overlay::{
            ui_transform, FrameMeta, OverlayBackend, OverlayRenderer, OverlayState, ShouldRender,
            SplitOverlayBackend,
        },
        task::TaskType,
    },
    graphics::CommandBuffers,
    state::{AppSession, AppState},
};

use super::screen::ScreenRenderer;
type PinnedSelectorFuture = core::pin::Pin<
    Box<dyn Future<Output = Result<PipewireSelectScreenResult, wlx_capture::pipewire::AshpdError>>>,
>;

pub struct MirrorRenderer {
    name: Arc<str>,
    renderer: Option<ScreenRenderer>,
    selector: Option<PinnedSelectorFuture>,
    last_extent: [u32; 3],
}
impl MirrorRenderer {
    pub fn new(name: Arc<str>) -> Self {
        let selector = Box::pin(pipewire_select_screen(None, false, false, false, false));
        Self {
            name,
            renderer: None,
            selector: Some(selector),
            last_extent: [0; 3],
        }
    }
}

impl OverlayRenderer for MirrorRenderer {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
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
        if let Some(mut selector) = self.selector.take() {
            let maybe_pw_result = match selector
                .poll_unpin(&mut Context::from_waker(futures::task::noop_waker_ref()))
            {
                Poll::Ready(result) => result,
                Poll::Pending => {
                    self.selector = Some(selector);
                    return Ok(false);
                }
            };

            match maybe_pw_result {
                Ok(pw_result) => {
                    let node_id = pw_result.streams.first().unwrap().node_id; // streams guaranteed to have at least one element
                    log::info!("{}: PipeWire node selected: {}", self.name.clone(), node_id);
                    let capture = PipewireCapture::new(self.name.clone(), node_id);
                    self.renderer = Some(ScreenRenderer::new_raw(
                        self.name.clone(),
                        Box::new(capture),
                    ));
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Name(self.name.clone()),
                        Box::new(|app, o| {
                            o.grabbable = true;
                            o.interactable = true;
                            o.reset(app, false);
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

        let mut result = false;
        if let Some(renderer) = self.renderer.as_mut() {
            result = renderer.render(app, tgt, buf, alpha)?;
            if let Some(meta) = renderer.frame_meta() {
                let extent = meta.extent;
                if self.last_extent != extent {
                    self.last_extent = extent;
                    // resized
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Name(self.name.clone()),
                        Box::new(move |_app, o| {
                            o.interaction_transform = ui_transform([extent[0], extent[1]]);
                        }),
                    ));
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
        Some(FrameMeta {
            extent: self.last_extent,
            ..Default::default()
        })
    }
}

pub fn new_mirror(
    name: Arc<str>,
    show_hide: bool,
    session: &AppSession,
) -> (OverlayState, Box<dyn OverlayBackend>) {
    let state = OverlayState {
        name: name.clone(),
        show_hide,
        want_visible: true,
        spawn_scale: 0.5 * session.config.desktop_view_scale,
        ..Default::default()
    };
    let backend = Box::new(SplitOverlayBackend {
        renderer: Box::new(MirrorRenderer::new(name)),
        ..Default::default()
    });

    (state, backend)
}
