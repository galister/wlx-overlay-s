use glam::{Affine2, Affine3A, Quat, Vec3, vec3};
use smithay::wayland::compositor::with_states;
use std::sync::Arc;
use vulkano::image::view::ImageView;
use wlx_capture::frame::MouseMeta;
use wlx_common::{
    overlays::{BackendAttrib, BackendAttribValue, StereoMode},
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    backend::{
        XrBackend,
        input::{self, HoverResult},
        wayvr::{self, SurfaceBufWithImage},
    },
    graphics::ExtentExt,
    overlays::screen::capture::ScreenPipeline,
    state::{self, AppState},
    subsystem::{hid::WheelDelta, input::KeyboardFocus},
    windowing::{
        backend::{
            FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender,
            ui_transform,
        },
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

pub fn create_wl_window_overlay(
    name: Arc<str>,
    xr_backend: XrBackend,
    window: wayvr::window::WindowHandle,
) -> OverlayWindowConfig {
    OverlayWindowConfig {
        name: name.clone(),
        default_state: OverlayWindowState {
            grabbable: true,
            interactable: true,
            positioning: Positioning::Floating,
            curvature: Some(0.15),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE,
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.95),
            ),
            ..OverlayWindowState::default()
        },
        keyboard_focus: Some(KeyboardFocus::WayVR),
        category: OverlayCategory::WayVR,
        show_on_spawn: true,
        ..OverlayWindowConfig::from_backend(Box::new(WvrWindowBackend::new(
            name, xr_backend, window,
        )))
    }
}

pub struct WvrWindowBackend {
    name: Arc<str>,
    pipeline: Option<ScreenPipeline>,
    interaction_transform: Option<Affine2>,
    window: wayvr::window::WindowHandle,
    just_resumed: bool,
    meta: Option<FrameMeta>,
    mouse: Option<MouseMeta>,
    stereo: Option<StereoMode>,
    cur_image: Option<Arc<ImageView>>,
}

impl WvrWindowBackend {
    const fn new(
        name: Arc<str>,
        xr_backend: XrBackend,
        window: wayvr::window::WindowHandle,
    ) -> Self {
        Self {
            name,
            pipeline: None,
            window,
            interaction_transform: None,
            just_resumed: false,
            meta: None,
            mouse: None,
            stereo: if matches!(xr_backend, XrBackend::OpenXR) {
                Some(StereoMode::None)
            } else {
                None
            },
            cur_image: None,
        }
    }
}

impl OverlayBackend for WvrWindowBackend {
    fn init(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn pause(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        self.just_resumed = true;
        Ok(())
    }

    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        let Some(toplevel) = app
            .wvr_server
            .as_ref()
            .and_then(|sv| sv.wm.windows.get(&self.window))
            .map(|win| win.toplevel.clone())
        else {
            log::debug!(
                "{:?}: WayVR overlay without matching window entry",
                self.name
            );
            return Ok(ShouldRender::Unable);
        };

        with_states(toplevel.wl_surface(), |states| {
            if let Some(surf) = SurfaceBufWithImage::get_from_surface(states) {
                let mut meta = FrameMeta {
                    extent: surf.image.image().extent(),
                    format: surf.image.format(),
                    ..Default::default()
                };

                if let Some(pipeline) = self.pipeline.as_mut() {
                    meta.extent[2] = pipeline.get_depth();
                    if self
                        .meta
                        .is_some_and(|old| old.extent[..2] != meta.extent[..2])
                    {
                        pipeline.set_extent(app, [meta.extent[0] as _, meta.extent[1] as _])?;
                        self.interaction_transform =
                            Some(ui_transform(meta.extent.extent_u32arr()));
                    }
                } else {
                    let pipeline =
                        ScreenPipeline::new(&meta, app, self.stereo.unwrap_or(StereoMode::None))?;
                    meta.extent[2] = pipeline.get_depth();
                    self.pipeline = Some(pipeline);
                    self.interaction_transform = Some(ui_transform(meta.extent.extent_u32arr()));
                }

                let mouse = app
                    .wvr_server
                    .as_ref()
                    .unwrap()
                    .wm
                    .mouse
                    .as_ref()
                    .filter(|m| m.hover_window == self.window)
                    .map(|m| MouseMeta {
                        x: (m.x as f32) / (meta.extent[0] as f32),
                        y: (m.y as f32) / (meta.extent[1] as f32),
                    });

                let mouse_dirty = self.mouse != mouse;
                self.mouse = mouse;
                self.meta = Some(meta);
                if self
                    .cur_image
                    .as_ref()
                    .is_none_or(|i| *i.image() != *surf.image.image())
                {
                    log::trace!(
                        "{}: new {} image",
                        self.name,
                        if surf.dmabuf { "DMA-buf" } else { "SHM" }
                    );
                    self.cur_image = Some(surf.image);
                    Ok(ShouldRender::Should)
                } else if mouse_dirty {
                    Ok(ShouldRender::Should)
                } else {
                    Ok(ShouldRender::Can)
                }
            } else {
                log::trace!("{}: no buffer for wl_surface", self.name);
                Ok(ShouldRender::Unable)
            }
        })
    }

    fn render(
        &mut self,
        app: &mut state::AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        let image = self.cur_image.as_ref().unwrap().clone();

        self.pipeline
            .as_mut()
            .unwrap()
            .render(image, self.mouse.as_ref(), app, rdr)?;

        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.meta
    }

    fn notify(
        &mut self,
        app: &mut state::AppState,
        event_data: OverlayEventData,
    ) -> anyhow::Result<()> {
        if let OverlayEventData::IdAssigned(oid) = event_data {
            let wvr_server = app.wvr_server.as_mut().unwrap(); //never None
            wvr_server.overlay_added(oid, self.window);
        }
        Ok(())
    }

    fn on_hover(&mut self, app: &mut state::AppState, hit: &input::PointerHit) -> HoverResult {
        if let Some(meta) = self.meta.as_ref() {
            let x = (hit.uv.x * (meta.extent[0] as f32)) as u32;
            let y = (hit.uv.y * (meta.extent[1] as f32)) as u32;

            let wvr_server = app.wvr_server.as_mut().unwrap(); //never None
            wvr_server.send_mouse_move(self.window, x, y);
        }

        HoverResult {
            haptics: None, // haptics are handled via task
            consume: true,
        }
    }

    fn on_left(&mut self, _app: &mut state::AppState, _pointer: usize) {
        // Ignore event
    }

    fn on_pointer(&mut self, app: &mut state::AppState, hit: &input::PointerHit, pressed: bool) {
        if let Some(index) = match hit.mode {
            input::PointerMode::Left => Some(wayvr::MouseIndex::Left),
            input::PointerMode::Middle => Some(wayvr::MouseIndex::Center),
            input::PointerMode::Right => Some(wayvr::MouseIndex::Right),
            _ => {
                // Unknown pointer event, ignore
                None
            }
        } {
            let wvr_server = app.wvr_server.as_mut().unwrap(); //never None
            if pressed {
                wvr_server.send_mouse_down(self.window, index);
            } else {
                wvr_server.send_mouse_up(index);
            }
        }
    }

    fn on_scroll(
        &mut self,
        app: &mut state::AppState,
        _hit: &input::PointerHit,
        delta: WheelDelta,
    ) {
        let wvr_server = app.wvr_server.as_mut().unwrap(); //never None
        wvr_server.send_mouse_scroll(delta);
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }

    fn get_attrib(&self, attrib: BackendAttrib) -> Option<BackendAttribValue> {
        match attrib {
            BackendAttrib::Stereo => self.stereo.map(BackendAttribValue::Stereo),
            _ => None,
        }
    }
    fn set_attrib(&mut self, app: &mut AppState, value: BackendAttribValue) -> bool {
        match value {
            BackendAttribValue::Stereo(new) => {
                if let Some(stereo) = self.stereo.as_mut() {
                    log::debug!("{}: stereo: {stereo:?} → {new:?}", self.name);
                    *stereo = new;
                    if let Some(pipeline) = self.pipeline.as_mut() {
                        pipeline.set_stereo(app, new).unwrap(); // only panics if gfx is dead
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
