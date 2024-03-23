use std::{
    sync::Arc,
    task::{Context, Poll},
};

use futures::{Future, FutureExt};
use glam::vec3a;
use wlx_capture::pipewire::{pipewire_select_screen, PipewireCapture, PipewireSelectScreenResult};

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
        overlay::{
            ui_transform, OverlayBackend, OverlayRenderer, OverlayState, SplitOverlayBackend,
        },
    },
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
        let selector = Box::pin(pipewire_select_screen(None, false, false, false));
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
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if let Some(mut selector) = self.selector.take() {
            let maybe_pw_result = match selector
                .poll_unpin(&mut Context::from_waker(futures::task::noop_waker_ref()))
            {
                Poll::Ready(result) => result,
                Poll::Pending => {
                    self.selector = Some(selector);
                    return Ok(());
                }
            };

            if let Ok(pw_result) = maybe_pw_result {
                log::info!(
                    "{}: PipeWire node selected: {}",
                    self.name.clone(),
                    pw_result.node_id
                );
                let capture = PipewireCapture::new(self.name.clone(), pw_result.node_id, 60);
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
            } else {
                log::warn!("Failed to create pipewire mirror");
                self.renderer = None;
                // drop self
                app.tasks
                    .enqueue(TaskType::DropOverlay(OverlaySelector::Name(
                        self.name.clone(),
                    )));
            }
        }

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.render(app)?;
            if let Some(view) = renderer.view() {
                let extent = view.image().extent();
                if self.last_extent != extent {
                    self.last_extent = extent;
                    // resized
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Name(self.name.clone()),
                        Box::new(move |_app, o| {
                            o.interaction_transform = ui_transform(&[extent[0], extent[1]]);
                        }),
                    ));
                }
            }
        }

        Ok(())
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
    fn view(&mut self) -> Option<std::sync::Arc<vulkano::image::view::ImageView>> {
        self.renderer.as_mut().and_then(|r| r.view())
    }
}

pub fn new_mirror(
    name: Arc<str>,
    show_hide: bool,
    session: &AppSession,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    let state = OverlayState {
        name: name.clone(),
        show_hide,
        want_visible: true,
        spawn_scale: 0.5 * session.config.desktop_view_scale,
        spawn_point: vec3a(0., 0.5, -0.5),
        ..Default::default()
    };
    let backend = Box::new(SplitOverlayBackend {
        renderer: Box::new(MirrorRenderer::new(name)),
        ..Default::default()
    });

    Some((state, backend))
}
