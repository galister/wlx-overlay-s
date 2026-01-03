use glam::{Affine2, Affine3A, Quat, Vec2, Vec3, vec2, vec3};
use smithay::{
    desktop::PopupManager,
    wayland::{compositor::with_states, shell::xdg::XdgPopupSurfaceData},
};
use std::sync::Arc;
use vulkano::{
    buffer::BufferUsage, image::view::ImageView, pipeline::graphics::color_blend::AttachmentBlend,
};
use wgui::{
    gfx::{
        cmd::WGfxClearMode,
        pipeline::{WGfxPipeline, WPipelineCreateInfo},
    },
    i18n::Translation,
    parser::Fetchable,
    widget::label::WidgetLabel,
};
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
    graphics::{ExtentExt, Vert2Uv, upload_quad_vertices},
    gui::panel::{GuiPanel, NewGuiPanelParams},
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

const BORDER_SIZE: u32 = 5;
const BAR_SIZE: u32 = 24;

pub fn create_wl_window_overlay(
    name: Arc<str>,
    app: &mut AppState,
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
                vec3(0.0, 0.0, -0.95),
            ),
            ..OverlayWindowState::default()
        },
        keyboard_focus: Some(KeyboardFocus::WayVR),
        category: OverlayCategory::WayVR,
        show_on_spawn: true,
        ..OverlayWindowConfig::from_backend(Box::new(WvrWindowBackend::new(name, app, window)?))
    })
}

pub struct WvrWindowBackend {
    name: Arc<str>,
    pipeline: Option<ScreenPipeline>,
    popups_pipeline: Arc<WGfxPipeline<Vert2Uv>>,
    interaction_transform: Option<Affine2>,
    window: wayvr::window::WindowHandle,
    popups: Vec<(Arc<ImageView>, Vec2)>,
    just_resumed: bool,
    meta: Option<FrameMeta>,
    mouse: Option<MouseMeta>,
    stereo: Option<StereoMode>,
    cur_image: Option<Arc<ImageView>>,
    panel: GuiPanel<()>,
    mouse_transform: Affine2,
}

impl WvrWindowBackend {
    fn new(
        name: Arc<str>,
        app: &mut AppState,
        window: wayvr::window::WindowHandle,
    ) -> anyhow::Result<Self> {
        let popups_pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_screen").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format).use_blend(AttachmentBlend::default()),
        )?;

        let mut panel = GuiPanel::new_from_template(
            app,
            "gui/decor.xml",
            (),
            NewGuiPanelParams {
                resize_to_parent: true,
                ..Default::default()
            },
        )?;

        {
            let mut title = panel
                .parser_state
                .fetch_widget_as::<WidgetLabel>(&panel.layout.state, "label_title")?;
            title.set_text_simple(
                &mut app.wgui_globals.get(),
                Translation::from_raw_text(&*name),
            );
        }

        panel.update_layout()?;

        Ok(Self {
            name,
            pipeline: None,
            window,
            popups: vec![],
            popups_pipeline,
            interaction_transform: None,
            just_resumed: false,
            meta: None,
            mouse: None,
            stereo: if matches!(app.xr_backend, XrBackend::OpenXR) {
                Some(StereoMode::None)
            } else {
                None
            },
            cur_image: None,
            panel,
            mouse_transform: Affine2::ZERO,
        })
    }
}

impl OverlayBackend for WvrWindowBackend {
    fn init(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.panel.init(app)
    }

    fn pause(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.panel.pause(app)
    }

    fn resume(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.just_resumed = true;
        self.panel.resume(app)
    }

    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        let should_render_panel = self.panel.should_render(app)?;

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

        let popups = PopupManager::popups_for_surface(toplevel.wl_surface())
            .filter_map(|(popup, point)| {
                with_states(popup.wl_surface(), |states| {
                    if !states
                        .data_map
                        .get::<XdgPopupSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .configured
                    {
                        // not yet configured
                        return None;
                    }

                    if let Some(surf) = SurfaceBufWithImage::get_from_surface(states) {
                        Some((surf.image, vec2(point.x as _, point.y as _)))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        with_states(toplevel.wl_surface(), |states| {
            if let Some(surf) = SurfaceBufWithImage::get_from_surface(states) {
                let mut meta = FrameMeta {
                    extent: surf.image.image().extent(),
                    format: surf.image.format(),
                    clear: WGfxClearMode::Clear([0.0, 0.0, 0.0, 0.0]),
                    ..Default::default()
                };

                if let Some(pipeline) = self.pipeline.as_mut() {
                    let inner_extent = meta.extent;

                    meta.extent[0] += BORDER_SIZE * 2;
                    meta.extent[1] += BORDER_SIZE * 2 + BAR_SIZE;
                    meta.extent[2] = pipeline.get_depth();
                    if self
                        .meta
                        .is_some_and(|old| old.extent[..2] != meta.extent[..2])
                    {
                        pipeline.set_extent(
                            app,
                            [inner_extent[0] as _, inner_extent[1] as _],
                            [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
                        )?;
                        self.interaction_transform =
                            Some(ui_transform(meta.extent.extent_u32arr()));
                    }
                } else {
                    let pipeline = ScreenPipeline::new(
                        &meta,
                        app,
                        self.stereo.unwrap_or(StereoMode::None),
                        [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
                    )?;
                    meta.extent[0] += BORDER_SIZE * 2;
                    meta.extent[1] += BORDER_SIZE * 2 + BAR_SIZE;
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

                let dirty = self.mouse != mouse || self.popups != popups;
                self.mouse = mouse;
                self.popups = popups;
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
                } else if dirty {
                    Ok(ShouldRender::Should)
                } else {
                    Ok(should_render_panel)
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
        self.panel.render(app, rdr)?;

        let image = self.cur_image.as_ref().unwrap().clone();

        self.pipeline
            .as_mut()
            .unwrap()
            .render(image, self.mouse.as_ref(), app, rdr)?;

        for (popup_img, point) in &self.popups {
            let extentf = self.meta.as_ref().unwrap().extent.extent_f32();
            let popup_extentf = popup_img.extent_f32();
            let mut buf_vert = app
                .gfx
                .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::VERTEX_BUFFER, 4)?;

            upload_quad_vertices(
                &mut buf_vert,
                extentf[0],
                extentf[1],
                point.x,
                point.y,
                popup_extentf[0],
                popup_extentf[1],
            )?;

            let set0 = self.popups_pipeline.uniform_sampler(
                0,
                popup_img.clone(),
                app.gfx.texture_filter,
            )?;
            let set1 = self
                .popups_pipeline
                .buffer(1, self.pipeline.as_ref().unwrap().get_alpha_buf())?;

            let pass = self.popups_pipeline.create_pass(
                extentf,
                [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
                buf_vert,
                0..4,
                0..1,
                vec![set0, set1],
                &Default::default(),
            )?;

            rdr.cmd_buf_single().run_ref(&pass)?;
        }

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

    fn on_left(&mut self, app: &mut state::AppState, pointer: usize) {
        self.panel.on_left(app, pointer);
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
