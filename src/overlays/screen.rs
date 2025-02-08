use core::slice;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    f32::consts::PI,
    ops::Add,
    ptr,
    sync::{atomic::AtomicU64, Arc},
    time::{Duration, Instant},
};
use vulkano::{
    command_buffer::CommandBufferUsage,
    image::{sampler::Filter, view::ImageView, Image},
    pipeline::graphics::color_blend::AttachmentBlend,
};
use wlx_capture::frame as wlx_frame;

use wlx_capture::{
    frame::{
        DrmFormat, FrameFormat, MouseMeta, WlxFrame, DRM_FORMAT_ABGR2101010, DRM_FORMAT_ABGR8888,
        DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR2101010, DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888,
    },
    WlxCapture,
};

#[cfg(feature = "pipewire")]
use {
    crate::config_io,
    std::error::Error,
    std::{ops::Deref, path::PathBuf, task},
    wlx_capture::pipewire::PipewireCapture,
    wlx_capture::pipewire::PipewireSelectScreenResult,
};

#[cfg(all(feature = "x11", feature = "pipewire"))]
use {crate::config::AStrMapExt, wlx_capture::pipewire::PipewireStream};

#[cfg(feature = "wayland")]
use {
    crate::config::AStrMapExt,
    wlx_capture::{
        wayland::{wayland_client::protocol::wl_output, WlxClient, WlxOutput},
        wlr_dmabuf::WlrDmabufCapture,
        wlr_screencopy::WlrScreencopyCapture,
    },
};

#[cfg(feature = "x11")]
use wlx_capture::xshm::{XshmCapture, XshmScreen};

use glam::{vec2, vec3a, Affine2, Affine3A, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{Haptics, InteractionHandler, PointerHit, PointerMode},
        overlay::{FrameTransform, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    config::{def_pw_tokens, GeneralConfig, PwTokenMap},
    graphics::{
        fourcc_to_vk, WlxCommandBuffer, WlxPipeline, WlxPipelineLegacy, DRM_FORMAT_MOD_INVALID,
    },
    hid::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    state::{AppSession, AppState, KeyboardFocus, ScreenMeta},
};

#[cfg(feature = "wayland")]
pub(crate) type WlxClientAlias = wlx_capture::wayland::WlxClient;

#[cfg(not(feature = "wayland"))]
pub(crate) type WlxClientAlias = ();

const CURSOR_SIZE: f32 = 16. / 1440.;

static DRM_FORMATS: once_cell::sync::OnceCell<Vec<DrmFormat>> = once_cell::sync::OnceCell::new();

static START: Lazy<Instant> = Lazy::new(Instant::now);
static NEXT_MOVE: AtomicU64 = AtomicU64::new(0);

fn can_move() -> bool {
    START.elapsed().as_millis() as u64 > NEXT_MOVE.load(std::sync::atomic::Ordering::Relaxed)
}

fn set_next_move(millis_from_now: u64) {
    NEXT_MOVE.store(
        START.elapsed().as_millis() as u64 + millis_from_now,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub struct ScreenInteractionHandler {
    next_scroll: Instant,
    mouse_transform: Affine2,
}
impl ScreenInteractionHandler {
    fn new(pos: Vec2, size: Vec2, transform: Transform) -> ScreenInteractionHandler {
        let transform = match transform {
            Transform::_90 | Transform::Flipped90 => Affine2::from_cols(
                vec2(0., size.y),
                vec2(-size.x, 0.),
                vec2(pos.x + size.x, pos.y),
            ),
            Transform::_180 | Transform::Flipped180 => Affine2::from_cols(
                vec2(-size.x, 0.),
                vec2(0., -size.y),
                vec2(pos.x + size.x, pos.y + size.y),
            ),
            Transform::_270 | Transform::Flipped270 => Affine2::from_cols(
                vec2(0., -size.y),
                vec2(size.x, 0.),
                vec2(pos.x, pos.y + size.y),
            ),
            _ => Affine2::from_cols(vec2(size.x, 0.), vec2(0., size.y), pos),
        };

        ScreenInteractionHandler {
            next_scroll: Instant::now(),
            mouse_transform: transform,
        }
    }
}

impl InteractionHandler for ScreenInteractionHandler {
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        #[cfg(debug_assertions)]
        log::trace!("Hover: {:?}", hit.uv);
        if can_move()
            && (!app.session.config.focus_follows_mouse_mode
                || app.input_state.pointers[hit.pointer].now.move_mouse)
        {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.hid_provider.mouse_move(pos);
            set_next_move(app.session.config.mouse_move_interval_ms as u64);
        }
        None
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let btn = match hit.mode {
            PointerMode::Right => MOUSE_RIGHT,
            PointerMode::Middle => MOUSE_MIDDLE,
            _ => MOUSE_LEFT,
        };

        if pressed {
            set_next_move(app.session.config.click_freeze_time_ms as u64);
        }

        app.hid_provider.send_button(btn, pressed);

        if !pressed {
            return;
        }
        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.hid_provider.mouse_move(pos);
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32) {
        if self.next_scroll > Instant::now() {
            return;
        }
        let max_millis = if matches!(hit.mode, PointerMode::Left) {
            200.0
        } else {
            100.0
        };

        let millis = (1. - delta.abs()) * max_millis;
        self.next_scroll = Instant::now().add(Duration::from_millis(millis as _));
        app.hid_provider.wheel(if delta < 0. { -1 } else { 1 })
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
}

#[derive(Clone)]
struct ScreenPipeline {
    view: Arc<ImageView>,
    mouse: Option<Arc<ImageView>>,
    pipeline: Arc<WlxPipeline<WlxPipelineLegacy>>,
    extentf: [f32; 2],
}

impl ScreenPipeline {
    fn new(extent: &[u32; 3], app: &mut AppState) -> anyhow::Result<ScreenPipeline> {
        let texture =
            app.graphics
                .render_texture(extent[0], extent[1], app.graphics.native_format)?;

        let view = ImageView::new_default(texture)?;

        let Ok(shaders) = app.graphics.shared_shaders.read() else {
            return Err(anyhow::anyhow!("Could not lock shared shaders for reading"));
        };

        let pipeline = app.graphics.create_pipeline(
            view.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_screen").unwrap().clone(), // want panic
            app.graphics.native_format,
            Some(AttachmentBlend::default()),
        )?;

        let extentf = [extent[0] as f32, extent[1] as f32];

        Ok(ScreenPipeline {
            view,
            mouse: None,
            pipeline,
            extentf,
        })
    }

    fn ensure_mouse_initialized(&mut self, uploads: &mut WlxCommandBuffer) -> anyhow::Result<()> {
        if self.mouse.is_some() {
            return Ok(());
        }

        #[rustfmt::skip]
        let mouse_bytes = [
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
        ];

        let mouse_tex =
            uploads.texture2d_raw(4, 4, vulkano::format::Format::R8G8B8A8_UNORM, &mouse_bytes)?;
        self.mouse = Some(ImageView::new_default(mouse_tex)?);
        Ok(())
    }

    fn render(
        &mut self,
        image: Arc<Image>,
        mouse: Option<&MouseMeta>,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        let mut cmd = app
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        let view = ImageView::new_default(image)?;
        let set0 = self
            .pipeline
            .uniform_sampler(0, view, app.graphics.texture_filtering)?;

        let pass = self.pipeline.create_pass(
            self.extentf,
            app.graphics.quad_verts.clone(),
            app.graphics.quad_indices.clone(),
            vec![set0],
        )?;

        cmd.begin_render_pass(&self.pipeline)?;
        cmd.run_ref(&pass)?;

        if let (Some(mouse), Some(mouse_view)) = (mouse, self.mouse.clone()) {
            let size = CURSOR_SIZE * self.extentf[1];
            let half_size = size * 0.5;

            let vertex_buffer = app.graphics.upload_verts(
                self.extentf[0],
                self.extentf[1],
                mouse.x * self.extentf[0] - half_size,
                mouse.y * self.extentf[1] - half_size,
                size,
                size,
            )?;

            let set0 = self
                .pipeline
                .uniform_sampler(0, mouse_view.clone(), Filter::Nearest)?;

            let pass = self.pipeline.create_pass(
                self.extentf,
                vertex_buffer,
                app.graphics.quad_indices.clone(),
                vec![set0],
            )?;

            cmd.run_ref(&pass)?;
        }

        cmd.end_render_pass()?;
        cmd.build_and_execute_now()
    }
}

pub struct ScreenRenderer {
    name: Arc<str>,
    capture: Box<dyn WlxCapture>,
    pipeline: Option<ScreenPipeline>,
    last_view: Option<Arc<ImageView>>,
    transform: Affine3A,
    extent: Option<[u32; 3]>,
}

impl ScreenRenderer {
    #[cfg(feature = "wayland")]
    pub fn new_raw(name: Arc<str>, capture: Box<dyn WlxCapture>) -> ScreenRenderer {
        ScreenRenderer {
            name,
            capture,
            pipeline: None,
            last_view: None,
            transform: Affine3A::IDENTITY,
            extent: None,
        }
    }

    #[cfg(feature = "wayland")]
    pub fn new_wlr_dmabuf(output: &WlxOutput) -> Option<ScreenRenderer> {
        let client = WlxClient::new()?;
        let capture = WlrDmabufCapture::new(client, output.id);

        Some(ScreenRenderer {
            name: output.name.clone(),
            capture: Box::new(capture),
            pipeline: None,
            last_view: None,
            transform: Affine3A::IDENTITY,
            extent: None,
        })
    }

    #[cfg(feature = "wayland")]
    pub fn new_wlr_screencopy(output: &WlxOutput) -> Option<ScreenRenderer> {
        let client = WlxClient::new()?;
        let capture = WlrScreencopyCapture::new(client, output.id);

        Some(ScreenRenderer {
            name: output.name.clone(),
            capture: Box::new(capture),
            pipeline: None,
            last_view: None,
            transform: Affine3A::IDENTITY,
            extent: None,
        })
    }

    #[cfg(feature = "wayland")]
    pub fn new_pw(
        output: &WlxOutput,
        token: Option<&str>,
        session: &AppSession,
    ) -> anyhow::Result<(
        ScreenRenderer,
        Option<String>, /* pipewire restore token */
    )> {
        let name = output.name.clone();
        let embed_mouse = !session.config.double_cursor_fix;

        let select_screen_result = select_pw_screen(
            &format!(
                "Now select: {} {} {} @ {},{}",
                &output.name,
                &output.make,
                &output.model,
                &output.logical_pos.0,
                &output.logical_pos.1
            ),
            token,
            embed_mouse,
            true,
            true,
            false,
        )?;

        let node_id = select_screen_result.streams.first().unwrap().node_id; // streams guaranteed to have at least one element

        let capture = PipewireCapture::new(name, node_id);

        Ok((
            ScreenRenderer {
                name: output.name.clone(),
                capture: Box::new(capture),
                pipeline: None,
                last_view: None,
                transform: Affine3A::IDENTITY,
                extent: None,
            },
            select_screen_result.restore_token,
        ))
    }

    #[cfg(feature = "x11")]
    pub fn new_xshm(screen: Arc<XshmScreen>) -> ScreenRenderer {
        let capture = XshmCapture::new(screen.clone());

        ScreenRenderer {
            name: screen.name.clone(),
            capture: Box::new(capture),
            pipeline: None,
            last_view: None,
            transform: Affine3A::IDENTITY,
            extent: None,
        }
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if !self.capture.is_ready() {
            let supports_dmabuf = app
                .graphics
                .device
                .enabled_extensions()
                .ext_external_memory_dma_buf
                && self.capture.supports_dmbuf();

            let allow_dmabuf = &*app.session.config.capture_method != "pw_fallback"
                && &*app.session.config.capture_method != "screencopy";

            let capture_method = app.session.config.capture_method.clone();

            let drm_formats = DRM_FORMATS.get_or_init({
                let graphics = app.graphics.clone();
                move || {
                    if !supports_dmabuf {
                        log::info!("Capture method does not support DMA-buf");
                        return vec![];
                    }
                    if !allow_dmabuf {
                        log::info!("Not using DMA-buf capture due to {}", capture_method);
                        return vec![];
                    }
                    log::warn!("Using DMA-buf capture. If screens are blank for you, switch to SHM using:");
                    log::warn!("echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml");

                    let possible_formats = [
                        DRM_FORMAT_ABGR8888.into(),
                        DRM_FORMAT_XBGR8888.into(),
                        DRM_FORMAT_ARGB8888.into(),
                        DRM_FORMAT_XRGB8888.into(),
                        DRM_FORMAT_ABGR2101010.into(),
                        DRM_FORMAT_XBGR2101010.into(),
                    ];

                    let mut final_formats = vec![];

                    for &f in &possible_formats {
                        let Ok(vk_fmt) = fourcc_to_vk(f) else {
                            continue;
                        };
                        let Ok(props) = graphics.device.physical_device().format_properties(vk_fmt)
                        else {
                            continue;
                        };
                        let mut fmt = DrmFormat {
                            fourcc: f,
                            modifiers: props
                                .drm_format_modifier_properties
                                .iter()
                                // important bit: only allow single-plane
                                .filter(|m| m.drm_format_modifier_plane_count == 1)
                                .map(|m| m.drm_format_modifier)
                                .collect(),
                        };
                        fmt.modifiers.push(DRM_FORMAT_MOD_INVALID); // implicit modifiers support
                        final_formats.push(fmt);
                    }
                    log::debug!("Supported DRM formats:");
                    for f in &final_formats {
                        log::debug!("  {} {:?}", f.fourcc, f.modifiers);
                    }
                    final_formats
                }
            });

            self.capture.init(drm_formats);
            self.capture.request_new_frame();
        };

        for frame in self.capture.receive().into_iter() {
            match frame {
                WlxFrame::Dmabuf(frame) => {
                    if !frame.is_valid() {
                        log::error!("Invalid frame");
                        continue;
                    }
                    self.extent.get_or_insert_with(|| {
                        extent_from_format(frame.format, &app.session.config)
                    });
                    if let Some(new_transform) = affine_from_format(&frame.format) {
                        self.transform = new_transform;
                    }
                    match app.graphics.dmabuf_texture(frame) {
                        Ok(new) => {
                            let pipeline = match self.pipeline {
                                Some(ref mut p) => Some(p),
                                None if app.session.config.screen_render_down => {
                                    log::info!("{}: Using render-down pass.", self.name);
                                    let pipeline = ScreenPipeline::new(&self.extent.unwrap(), app)?; // safe
                                    self.last_view = Some(pipeline.view.clone());
                                    self.pipeline = Some(pipeline);
                                    self.pipeline.as_mut()
                                }
                                None => None,
                            };
                            if let Some(pipeline) = pipeline {
                                pipeline.render(new.clone(), None, app)?;
                            } else {
                                let view = ImageView::new_default(new.clone())?;
                                self.last_view = Some(view);
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "{}: Failed to create DMA-buf texture: {}",
                                self.name,
                                e.to_string()
                            );
                        }
                    }
                    self.capture.request_new_frame();
                }
                WlxFrame::MemFd(frame) => {
                    let mut upload = app
                        .graphics
                        .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

                    let Some(fd) = frame.plane.fd else {
                        log::error!("No fd");
                        continue;
                    };
                    self.extent.get_or_insert_with(|| {
                        extent_from_format(frame.format, &app.session.config)
                    });
                    if let Some(new_transform) = affine_from_format(&frame.format) {
                        self.transform = new_transform;
                    }
                    log::debug!("{}: New MemFd frame", self.name);
                    let format = fourcc_to_vk(frame.format.fourcc)?;

                    let len = frame.plane.stride as usize * frame.format.height as usize;
                    let offset = frame.plane.offset as i64;

                    let map = unsafe {
                        libc::mmap(
                            ptr::null_mut(),
                            len,
                            libc::PROT_READ,
                            libc::MAP_SHARED,
                            fd,
                            offset,
                        )
                    } as *const u8;

                    let data = unsafe { slice::from_raw_parts(map, len) };

                    let image = upload.texture2d_raw(
                        frame.format.width,
                        frame.format.height,
                        format,
                        data,
                    )?;
                    upload.build_and_execute_now()?;

                    unsafe { libc::munmap(map as *mut _, len) };

                    self.last_view = Some(ImageView::new_default(image)?);
                    self.capture.request_new_frame();
                }
                WlxFrame::MemPtr(frame) => {
                    log::debug!("{}: New MemPtr frame", self.name);
                    let mut upload = app
                        .graphics
                        .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

                    let format = fourcc_to_vk(frame.format.fourcc)?;

                    let data = unsafe { slice::from_raw_parts(frame.ptr as *const u8, frame.size) };

                    let image = upload.texture2d_raw(
                        frame.format.width,
                        frame.format.height,
                        format,
                        data,
                    )?;

                    let pipeline = Some(match self.pipeline {
                        Some(ref mut p) => p,
                        _ => {
                            log::info!("{}: Using render-down pass.", self.name);
                            let extent = extent_from_format(frame.format, &app.session.config);
                            let mut pipeline = ScreenPipeline::new(&extent, app)?;
                            self.last_view = Some(pipeline.view.clone());
                            pipeline.ensure_mouse_initialized(&mut upload)?;
                            self.pipeline = Some(pipeline);
                            self.extent = Some(extent);
                            self.pipeline.as_mut().unwrap() // safe
                        }
                    });

                    upload.build_and_execute_now()?;

                    if let Some(pipeline) = pipeline {
                        pipeline.render(image, frame.mouse.as_ref(), app)?;
                    } else {
                        let view = ImageView::new_default(image)?;
                        self.last_view = Some(view);
                    }
                    self.capture.request_new_frame();
                }
            };
        }
        Ok(())
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.pause();
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.resume();
        Ok(())
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        self.last_view.clone()
    }
    fn frame_transform(&mut self) -> Option<FrameTransform> {
        self.extent.map(|extent| FrameTransform {
            extent,
            transform: self.transform,
        })
    }
}

#[cfg(feature = "wayland")]
pub fn create_screen_renderer_wl(
    output: &WlxOutput,
    has_wlr_dmabuf: bool,
    has_wlr_screencopy: bool,
    pw_token_store: &mut PwTokenMap,
    session: &AppSession,
) -> Option<ScreenRenderer> {
    let mut capture: Option<ScreenRenderer> = None;
    if (&*session.config.capture_method == "wlr-dmabuf") && has_wlr_dmabuf {
        log::info!("{}: Using Wlr DMA-Buf", &output.name);
        capture = ScreenRenderer::new_wlr_dmabuf(output);
    }

    if &*session.config.capture_method == "screencopy" && has_wlr_screencopy {
        log::info!("{}: Using Wlr Screencopy Wl-SHM", &output.name);
        capture = ScreenRenderer::new_wlr_screencopy(output);
    }

    if capture.is_none() {
        log::info!("{}: Using Pipewire capture", &output.name);

        let display_name = output.name.deref();

        // Find existing token by display
        let token = pw_token_store.arc_get(display_name).map(|s| s.as_str());

        if let Some(t) = token {
            log::info!(
                "Found existing Pipewire token for display {}: {}",
                display_name,
                t
            );
        }

        match ScreenRenderer::new_pw(output, token, session) {
            Ok((renderer, restore_token)) => {
                capture = Some(renderer);

                if let Some(token) = restore_token {
                    if pw_token_store.arc_set(display_name.into(), token.clone()) {
                        log::info!("Adding Pipewire token {}", token);
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "{}: Failed to create Pipewire capture: {:?}",
                    &output.name,
                    e
                );
            }
        }
    }
    capture
}

pub fn create_screen_interaction(
    logical_pos: Vec2,
    logical_size: Vec2,
    transform: Transform,
) -> ScreenInteractionHandler {
    ScreenInteractionHandler::new(logical_pos, logical_size, transform)
}

fn create_screen_state(
    name: Arc<str>,
    res: (i32, i32),
    transform: Transform,
    session: &AppSession,
) -> OverlayState {
    let angle = if session.config.upright_screen_fix {
        match transform {
            Transform::_90 | Transform::Flipped90 => PI / 2.,
            Transform::_180 | Transform::Flipped180 => PI,
            Transform::_270 | Transform::Flipped270 => -PI / 2.,
            _ => 0.,
        }
    } else {
        0.
    };

    let center = Vec2 { x: 0.5, y: 0.5 };
    let interaction_transform = match transform {
        Transform::_90 | Transform::Flipped90 => Affine2::from_cols(
            Vec2::NEG_Y * (res.0 as f32 / res.1 as f32),
            Vec2::NEG_X,
            center,
        ),
        Transform::_180 | Transform::Flipped180 => Affine2::from_cols(
            Vec2::NEG_X,
            Vec2::NEG_Y * (-res.0 as f32 / res.1 as f32),
            center,
        ),
        Transform::_270 | Transform::Flipped270 => {
            Affine2::from_cols(Vec2::Y * (res.0 as f32 / res.1 as f32), Vec2::X, center)
        }
        _ if res.1 > res.0 => {
            // Xorg upright screens
            Affine2::from_cols(Vec2::X * (res.1 as f32 / res.0 as f32), Vec2::NEG_Y, center)
        }
        _ => Affine2::from_cols(Vec2::X, Vec2::NEG_Y * (res.0 as f32 / res.1 as f32), center),
    };

    OverlayState {
        name: name.clone(),
        keyboard_focus: Some(KeyboardFocus::PhysicalScreen),
        grabbable: true,
        recenter: true,
        anchored: true,
        interactable: true,
        spawn_scale: 1.5 * session.config.desktop_view_scale,
        spawn_point: vec3a(0., 0.5, 0.),
        spawn_rotation: Quat::from_axis_angle(Vec3::Z, angle),
        interaction_transform,
        ..Default::default()
    }
}

#[derive(Deserialize, Serialize, Default)]
pub struct TokenConf {
    #[serde(default = "def_pw_tokens")]
    pub pw_tokens: PwTokenMap,
}

#[cfg(feature = "pipewire")]
fn get_pw_token_path() -> PathBuf {
    let mut path = config_io::ConfigRoot::Generic.get_conf_d_path();
    path.push("pw_tokens.yaml");
    path
}

#[cfg(feature = "pipewire")]
pub fn save_pw_token_config(tokens: PwTokenMap) -> Result<(), Box<dyn Error>> {
    let conf = TokenConf { pw_tokens: tokens };
    let yaml = serde_yaml::to_string(&conf)?;
    std::fs::write(get_pw_token_path(), yaml)?;

    Ok(())
}

#[cfg(feature = "pipewire")]
pub fn load_pw_token_config() -> Result<PwTokenMap, Box<dyn Error>> {
    let yaml = std::fs::read_to_string(get_pw_token_path())?;
    let conf: TokenConf = serde_yaml::from_str(yaml.as_str())?;
    Ok(conf.pw_tokens)
}

pub(crate) struct ScreenCreateData {
    pub screens: Vec<(ScreenMeta, OverlayState, Box<SplitOverlayBackend>)>,
}

#[cfg(not(feature = "wayland"))]
pub fn create_screens_wayland(
    _wl: &mut WlxClientAlias,
    _app: &AppState,
) -> anyhow::Result<ScreenCreateData> {
    anyhow::bail!("Wayland support not enabled")
}

#[cfg(feature = "wayland")]
pub fn create_screens_wayland(
    wl: &mut WlxClientAlias,
    app: &mut AppState,
) -> anyhow::Result<ScreenCreateData> {
    let mut screens = vec![];

    // Load existing Pipewire tokens from file
    let mut pw_tokens: PwTokenMap = load_pw_token_config().unwrap_or_default();

    let pw_tokens_copy = pw_tokens.clone();
    let has_wlr_dmabuf = wl.maybe_wlr_dmabuf_mgr.is_some();
    let has_wlr_screencopy = wl.maybe_wlr_screencopy_mgr.is_some();

    for (id, output) in wl.outputs.iter() {
        if app.screens.iter().any(|s| s.name == output.name) {
            continue;
        }

        log::info!(
            "{}: Init screen of res {:?}, logical {:?} at {:?}",
            output.name,
            output.size,
            output.logical_size,
            output.logical_pos,
        );

        if let Some(renderer) = create_screen_renderer_wl(
            output,
            has_wlr_dmabuf,
            has_wlr_screencopy,
            &mut pw_tokens,
            &app.session,
        ) {
            let logical_pos = vec2(output.logical_pos.0 as f32, output.logical_pos.1 as f32);
            let logical_size = vec2(output.logical_size.0 as f32, output.logical_size.1 as f32);
            let transform = output.transform.into();
            let interaction = create_screen_interaction(logical_pos, logical_size, transform);
            let state =
                create_screen_state(output.name.clone(), output.size, transform, &app.session);

            let meta = ScreenMeta {
                name: wl.outputs[id].name.clone(),
                id: state.id,
                native_handle: *id,
            };

            let backend = Box::new(SplitOverlayBackend {
                renderer: Box::new(renderer),
                interaction: Box::new(interaction),
            });
            screens.push((meta, state, backend));
        }
    }

    if pw_tokens_copy != pw_tokens {
        // Token list changed, re-create token config file
        if let Err(err) = save_pw_token_config(pw_tokens) {
            log::error!("Failed to save Pipewire token config: {}", err);
        }
    }

    let extent = wl.get_desktop_extent();
    let origin = wl.get_desktop_origin();

    app.hid_provider
        .set_desktop_extent(vec2(extent.0 as f32, extent.1 as f32));
    app.hid_provider
        .set_desktop_origin(vec2(origin.0 as f32, origin.1 as f32));

    Ok(ScreenCreateData { screens })
}

#[cfg(not(feature = "x11"))]
pub fn create_screens_xshm(_app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    anyhow::bail!("X11 support not enabled")
}

#[cfg(not(all(feature = "x11", feature = "pipewire")))]
pub fn create_screens_x11pw(_app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    anyhow::bail!("Pipewire support not enabled")
}

#[cfg(all(feature = "x11", feature = "pipewire"))]
pub fn create_screens_x11pw(app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    use anyhow::bail;

    // Load existing Pipewire tokens from file
    let mut pw_tokens: PwTokenMap = load_pw_token_config().unwrap_or_default();
    let pw_tokens_copy = pw_tokens.clone();
    let token = pw_tokens.arc_get("x11").map(|s| s.as_str());
    let embed_mouse = !app.session.config.double_cursor_fix;

    let select_screen_result = select_pw_screen(
        "Select ALL screens on the screencast pop-up!",
        token,
        embed_mouse,
        true,
        true,
        true,
    )?;

    if let Some(restore_token) = select_screen_result.restore_token {
        if pw_tokens.arc_set("x11".into(), restore_token.clone()) {
            log::info!("Adding Pipewire token {}", restore_token);
        }
    }
    if pw_tokens_copy != pw_tokens {
        // Token list changed, re-create token config file
        if let Err(err) = save_pw_token_config(pw_tokens) {
            log::error!("Failed to save Pipewire token config: {}", err);
        }
    }

    let monitors = match XshmCapture::get_monitors() {
        Ok(m) => m,
        Err(e) => {
            bail!(e.to_string());
        }
    };
    log::info!("Got {} monitors", monitors.len());
    log::info!("Got {} streams", select_screen_result.streams.len());

    let mut extent = vec2(0., 0.);
    let screens = select_screen_result
        .streams
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let m = best_match(&s, monitors.iter().map(AsRef::as_ref)).unwrap();
            log::info!("Stream {i} is {}", m.name);
            extent.x = extent.x.max((m.monitor.x() + m.monitor.width()) as f32);
            extent.y = extent.y.max((m.monitor.y() + m.monitor.height()) as f32);

            let size = (m.monitor.width(), m.monitor.height());
            let interaction = create_screen_interaction(
                vec2(m.monitor.x() as f32, m.monitor.y() as f32),
                vec2(m.monitor.width() as f32, m.monitor.height() as f32),
                Transform::Normal,
            );

            let state = create_screen_state(m.name.clone(), size, Transform::Normal, &app.session);

            let meta = ScreenMeta {
                name: m.name.clone(),
                id: state.id,
                native_handle: 0,
            };

            let renderer = ScreenRenderer {
                name: m.name.clone(),
                capture: Box::new(PipewireCapture::new(m.name.clone(), s.node_id)),
                pipeline: None,
                last_view: None,
                transform: Affine3A::IDENTITY,
                extent: Some(extent_from_res(
                    size.0 as _,
                    size.1 as _,
                    &app.session.config,
                )),
            };

            let backend = Box::new(SplitOverlayBackend {
                renderer: Box::new(renderer),
                interaction: Box::new(interaction),
            });
            (meta, state, backend)
        })
        .collect();

    app.hid_provider.set_desktop_extent(extent);
    app.hid_provider.set_desktop_origin(vec2(0.0, 0.0));

    Ok(ScreenCreateData { screens })
}

#[cfg(feature = "x11")]
pub fn create_screens_xshm(app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    use anyhow::bail;

    let mut extent = vec2(0., 0.);

    let monitors = match XshmCapture::get_monitors() {
        Ok(m) => m,
        Err(e) => {
            bail!(e.to_string());
        }
    };

    let screens = monitors
        .into_iter()
        .map(|s| {
            extent.x = extent.x.max((s.monitor.x() + s.monitor.width()) as f32);
            extent.y = extent.y.max((s.monitor.y() + s.monitor.height()) as f32);

            let size = (s.monitor.width(), s.monitor.height());
            let pos = (s.monitor.x(), s.monitor.y());
            let renderer = ScreenRenderer::new_xshm(s.clone());

            log::info!(
                "{}: Init X11 screen of res {:?} at {:?}",
                s.name.clone(),
                size,
                pos,
            );

            let interaction = create_screen_interaction(
                vec2(s.monitor.x() as f32, s.monitor.y() as f32),
                vec2(size.0 as f32, size.1 as f32),
                Transform::Normal,
            );

            let state = create_screen_state(s.name.clone(), size, Transform::Normal, &app.session);

            let meta = ScreenMeta {
                name: s.name.clone(),
                id: state.id,
                native_handle: 0,
            };

            let backend = Box::new(SplitOverlayBackend {
                renderer: Box::new(renderer),
                interaction: Box::new(interaction),
            });
            (meta, state, backend)
        })
        .collect();

    app.hid_provider.set_desktop_extent(extent);
    app.hid_provider.set_desktop_origin(vec2(0.0, 0.0));

    Ok(ScreenCreateData { screens })
}

#[allow(unused)]
#[derive(Clone, Copy)]
pub enum Transform {
    Normal,
    _90,
    _180,
    _270,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[cfg(feature = "wayland")]
impl From<wl_output::Transform> for Transform {
    fn from(t: wl_output::Transform) -> Transform {
        match t {
            wl_output::Transform::Normal => Transform::Normal,
            wl_output::Transform::_90 => Transform::_90,
            wl_output::Transform::_180 => Transform::_180,
            wl_output::Transform::_270 => Transform::_270,
            wl_output::Transform::Flipped => Transform::Flipped180,
            wl_output::Transform::Flipped90 => Transform::Flipped90,
            wl_output::Transform::Flipped180 => Transform::Flipped180,
            wl_output::Transform::Flipped270 => Transform::Flipped270,
            _ => Transform::Normal,
        }
    }
}

fn extent_from_format(fmt: FrameFormat, config: &GeneralConfig) -> [u32; 3] {
    extent_from_res(fmt.width, fmt.height, config)
}

fn extent_from_res(width: u32, height: u32, config: &GeneralConfig) -> [u32; 3] {
    // screens above a certain resolution will have severe aliasing
    let h = height.min(config.screen_max_height as u32);
    let w = (width as f32 / height as f32 * h as f32) as u32;
    [w, h, 1]
}

fn affine_from_format(format: &FrameFormat) -> Option<Affine3A> {
    const FLIP_X: Vec3 = Vec3 {
        x: -1.0,
        y: 1.0,
        z: 1.0,
    };

    Some(match format.transform {
        wlx_frame::Transform::Normal => Affine3A::IDENTITY,
        wlx_frame::Transform::Rotated90 => Affine3A::from_rotation_z(-PI / 2.0),
        wlx_frame::Transform::Rotated180 => Affine3A::from_rotation_z(PI),
        wlx_frame::Transform::Rotated270 => Affine3A::from_rotation_z(PI / 2.0),
        wlx_frame::Transform::Flipped => Affine3A::from_scale(FLIP_X),
        wlx_frame::Transform::Flipped90 => {
            Affine3A::from_scale(FLIP_X) * Affine3A::from_rotation_z(-PI / 2.0)
        }
        wlx_frame::Transform::Flipped180 => {
            Affine3A::from_scale(FLIP_X) * Affine3A::from_rotation_z(PI)
        }
        wlx_frame::Transform::Flipped270 => {
            Affine3A::from_scale(FLIP_X) * Affine3A::from_rotation_z(PI / 2.0)
        }
        wlx_frame::Transform::Undefined => return None,
    })
}

#[cfg(all(feature = "pipewire", feature = "x11"))]
fn best_match<'a>(
    stream: &PipewireStream,
    mut streams: impl Iterator<Item = &'a XshmScreen>,
) -> Option<&'a XshmScreen> {
    let mut best = streams.next();
    log::debug!("stream: {:?}", stream.position);
    log::debug!("first: {:?}", best.map(|b| &b.monitor));
    let Some(position) = stream.position else {
        return best;
    };

    let mut best_dist = best
        .map(|b| (b.monitor.x() - position.0).abs() + (b.monitor.y() - position.1).abs())
        .unwrap_or(i32::MAX);
    for stream in streams {
        log::debug!("checking: {:?}", stream.monitor);
        let dist =
            (stream.monitor.x() - position.0).abs() + (stream.monitor.y() - position.1).abs();
        if dist < best_dist {
            best = Some(stream);
            best_dist = dist;
        }
    }
    log::debug!("best: {:?}", best.map(|b| &b.monitor));
    best
}

#[cfg(feature = "pipewire")]
fn select_pw_screen(
    instructions: &str,
    token: Option<&str>,
    embed_mouse: bool,
    screens_only: bool,
    persist: bool,
    multiple: bool,
) -> Result<PipewireSelectScreenResult, wlx_capture::pipewire::AshpdError> {
    use crate::backend::notifications::DbusNotificationSender;
    use wlx_capture::pipewire::pipewire_select_screen;

    let future = async move {
        let print_at = Instant::now() + Duration::from_millis(250);
        let mut notify = None;

        let f = pipewire_select_screen(token, embed_mouse, screens_only, persist, multiple);
        futures::pin_mut!(f);

        loop {
            match futures::poll!(&mut f) {
                task::Poll::Ready(result) => return result,
                task::Poll::Pending => {
                    if Instant::now() >= print_at {
                        log::info!("{}", instructions);
                        if let Ok(sender) = DbusNotificationSender::new() {
                            if let Ok(id) = sender.notify_send(instructions, "", 2, 0, 0, true) {
                                notify = Some((sender, id));
                            }
                        }
                        break;
                    }
                    futures::future::lazy(|_| {
                        std::thread::sleep(Duration::from_millis(10));
                    })
                    .await;
                    continue;
                }
            }
        }

        let result = f.await;
        if let Some((sender, id)) = notify {
            let _ = sender.notify_close(id);
        }
        result
    };

    futures::executor::block_on(future)
}
