use glam::{Affine2, Affine3A, Quat, Vec3, vec3};
use smithay::wayland::compositor::with_states;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::image::view::ImageView;
use wgui::gfx::WGfx;
use wlx_common::{
    overlays::{BackendAttrib, BackendAttribValue, StereoMode},
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    backend::{
        XrBackend,
        input::{self, HoverResult},
        wayvr::{self, SurfaceBufWithImage, WayVR, window::WindowManager},
    },
    graphics::{ExtentExt, WGfxExtras},
    ipc::{event_queue::SyncEventQueue, signal::WayVRSignal},
    overlays::screen::capture::ScreenPipeline,
    state::{self, AppState},
    subsystem::{hid::WheelDelta, input::KeyboardFocus},
    windowing::{
        OverlayID,
        backend::{
            FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender,
            ui_transform,
        },
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

pub struct WayVRData {
    pub window_handle_map: HashMap<wayvr::window::WindowHandle, OverlayID>,
    pub data: WayVR,
}

impl WayVRData {
    pub fn new(
        gfx: Arc<WGfx>,
        gfx_extras: &WGfxExtras,
        config: wayvr::Config,
        signals: SyncEventQueue<WayVRSignal>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            window_handle_map: HashMap::default(),
            data: WayVR::new(gfx, &gfx_extras, config, signals)?,
        })
    }
}

pub fn create_wl_window_overlay(
    name: Arc<str>,
    xr_backend: XrBackend,
    wayvr: Rc<RefCell<WayVRData>>,
    window: wayvr::window::WindowHandle,
) -> anyhow::Result<OverlayWindowConfig> {
    Ok(OverlayWindowConfig {
        name: name.clone(),
        default_state: OverlayWindowState {
            grabbable: true,
            interactable: true,
            positioning: Positioning::Floating,
            curvature: Some(0.15),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE,
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.4),
            ),
            ..OverlayWindowState::default()
        },
        keyboard_focus: Some(KeyboardFocus::WayVR),
        category: OverlayCategory::WayVR,
        show_on_spawn: true,
        ..OverlayWindowConfig::from_backend(Box::new(WayVRBackend::new(
            name, xr_backend, wayvr, window,
        )?))
    })
}

pub struct WayVRBackend {
    name: Arc<str>,
    pipeline: Option<ScreenPipeline>,
    mouse_transform: Affine2,
    interaction_transform: Option<Affine2>,
    window: wayvr::window::WindowHandle,
    wayvr: Rc<RefCell<WayVRData>>,
    wm: Rc<RefCell<WindowManager>>,
    just_resumed: bool,
    meta: Option<FrameMeta>,
    stereo: Option<StereoMode>,
    cur_image: Option<Arc<ImageView>>,
}

impl WayVRBackend {
    fn new(
        name: Arc<str>,
        xr_backend: XrBackend,
        wayvr: Rc<RefCell<WayVRData>>,
        window: wayvr::window::WindowHandle,
    ) -> anyhow::Result<Self> {
        let wm = wayvr.borrow().data.state.wm.clone();
        Ok(Self {
            name,
            pipeline: None,
            wayvr,
            wm,
            window,
            mouse_transform: Affine2::IDENTITY,
            interaction_transform: None,
            just_resumed: false,
            meta: None,
            stereo: if matches!(xr_backend, XrBackend::OpenXR) {
                Some(StereoMode::None)
            } else {
                None
            },
            cur_image: None,
        })
    }
}

impl OverlayBackend for WayVRBackend {
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
        let wm = self.wm.borrow();
        let Some(window) = wm.windows.get(&self.window) else {
            log::debug!(
                "{:?}: WayVR overlay without matching window entry",
                self.name
            );
            return Ok(ShouldRender::Unable);
        };

        with_states(window.toplevel.wl_surface(), |states| {
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

                self.meta = Some(meta);
                if self
                    .cur_image
                    .as_ref()
                    .is_none_or(|i| *i.image() != *surf.image.image())
                {
                    self.cur_image = Some(surf.image);
                    Ok(ShouldRender::Should)
                } else {
                    Ok(ShouldRender::Can)
                }
            } else {
                Ok(ShouldRender::Unable)
            }
        })
    }

    fn render(
        &mut self,
        app: &mut state::AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        let mouse = None; //TODO: mouse cursor
        let image = self.cur_image.as_ref().unwrap().clone();

        self.pipeline
            .as_mut()
            .unwrap()
            .render(image, mouse, app, rdr)?;

        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.meta
    }

    fn notify(
        &mut self,
        _app: &mut state::AppState,
        _event_data: OverlayEventData,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_hover(&mut self, _app: &mut state::AppState, hit: &input::PointerHit) -> HoverResult {
        if let Some(window) = self.wm.borrow().windows.get(&self.window) {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            let x = ((pos.x * (window.size_x as f32)) as u32).max(0);
            let y = ((pos.y * (window.size_y as f32)) as u32).max(0);

            let wayvr = &mut self.wayvr.borrow_mut().data;
            wayvr.state.send_mouse_move(self.window, x, y);
        }

        HoverResult {
            haptics: None, // haptics are handled via task
            consume: true,
        }
    }

    fn on_left(&mut self, _app: &mut state::AppState, _pointer: usize) {
        // Ignore event
    }

    fn on_pointer(&mut self, _app: &mut state::AppState, hit: &input::PointerHit, pressed: bool) {
        if let Some(index) = match hit.mode {
            input::PointerMode::Left => Some(wayvr::MouseIndex::Left),
            input::PointerMode::Middle => Some(wayvr::MouseIndex::Center),
            input::PointerMode::Right => Some(wayvr::MouseIndex::Right),
            _ => {
                // Unknown pointer event, ignore
                None
            }
        } {
            let wayvr = &mut self.wayvr.borrow_mut().data;
            if pressed {
                wayvr.state.send_mouse_down(self.window, index);
            } else {
                wayvr.state.send_mouse_up(index);
            }
        }
    }

    fn on_scroll(
        &mut self,
        _app: &mut state::AppState,
        _hit: &input::PointerHit,
        delta: WheelDelta,
    ) {
        self.wayvr.borrow_mut().data.state.send_mouse_scroll(delta);
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }

    fn get_attrib(&self, attrib: BackendAttrib) -> Option<BackendAttribValue> {
        match attrib {
            BackendAttrib::Stereo => self.stereo.map(|s| BackendAttribValue::Stereo(s)),
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
