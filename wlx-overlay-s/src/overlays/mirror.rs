use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use futures::{Future, FutureExt};
use glam::{Affine2, Affine3A, Quat, Vec3, vec3};
use wlx_capture::pipewire::{PipewireCapture, PipewireSelectScreenResult, pipewire_select_screen};
use wlx_common::windowing::OverlayWindowState;

use crate::{
    backend::{
        input::{HoverResult, PointerHit},
        task::{OverlayTask, TaskType},
    },
    state::{AppSession, AppState},
    subsystem::hid::WheelDelta,
    windowing::{
        OverlaySelector,
        backend::{
            BackendAttrib, BackendAttribValue, FrameMeta, OverlayBackend, OverlayEventData,
            RenderResources, ShouldRender, ui_transform,
        },
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

use super::screen::backend::ScreenBackend;
type PinnedSelectorFuture = core::pin::Pin<
    Box<dyn Future<Output = Result<PipewireSelectScreenResult, wlx_capture::pipewire::AshpdError>>>,
>;

static MIRROR_COUNTER: AtomicUsize = AtomicUsize::new(1);

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
                    app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
                        OverlaySelector::Name(self.name.clone()),
                        Box::new(|app, o| {
                            o.activate(app);
                        }),
                    )));
                }
                Err(e) => {
                    log::warn!("Failed to create mirror due to PipeWire error: {e:?}");
                    self.renderer = None;
                    // drop self
                    app.tasks
                        .enqueue(TaskType::Overlay(OverlayTask::Drop(OverlaySelector::Name(
                            self.name.clone(),
                        ))));
                }
            }
        }
        self.renderer
            .as_mut()
            .map_or(Ok(ShouldRender::Unable), |r| r.should_render(app))
    }
    fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()> {
        let Some(renderer) = self.renderer.as_mut() else {
            anyhow::bail!("render failed after should_render passed");
        };

        renderer.render(app, rdr)?;
        if let Some(meta) = renderer.frame_meta() {
            let extent = meta.extent;
            if self.last_extent != extent {
                self.last_extent = extent;
                self.interaction_transform = Some(ui_transform([extent[0], extent[1]]));
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

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.renderer.as_mut().and_then(ScreenBackend::frame_meta)
    }

    fn notify(&mut self, app: &mut AppState, event_data: OverlayEventData) -> anyhow::Result<()> {
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        renderer.notify(app, event_data)
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
    fn get_attrib(&self, attrib: BackendAttrib) -> Option<BackendAttribValue> {
        if let Some(renderer) = self.renderer.as_ref() {
            renderer.get_attrib(attrib)
        } else {
            None
        }
    }
    fn set_attrib(&mut self, app: &mut AppState, value: BackendAttribValue) -> bool {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_attrib(app, value)
        } else {
            false
        }
    }
}

pub fn new_mirror_name() -> Arc<str> {
    format!("M-{}", MIRROR_COUNTER.fetch_add(1, Ordering::Relaxed)).into()
}

pub fn new_mirror(name: Arc<str>, session: &AppSession) -> OverlayWindowConfig {
    OverlayWindowConfig {
        name: name.clone(),
        category: OverlayCategory::Mirror,
        show_on_spawn: true,
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            curvature: Some(0.15),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * session.config.desktop_view_scale,
                Quat::IDENTITY,
                vec3(0.0, 0.2, -0.35),
            ),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(MirrorBackend::new(name)))
    }
}
