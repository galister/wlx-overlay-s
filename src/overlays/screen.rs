use core::slice;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    error::Error,
    f32::consts::PI,
    ops::{Add, Deref},
    path::PathBuf,
    ptr,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};
use vulkano::{
    command_buffer::CommandBufferUsage,
    image::{sampler::Filter, view::ImageView, Image},
};
use wlx_capture::{
    frame::{
        DrmFormat, MouseMeta, WlxFrame, DRM_FORMAT_ABGR8888, DRM_FORMAT_ARGB8888,
        DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888,
    },
    pipewire::{pipewire_select_screen, PipewireCapture},
    wayland::{wayland_client::protocol::wl_output::Transform, WlxClient, WlxOutput},
    wlr_dmabuf::WlrDmabufCapture,
    xshm::{XshmCapture, XshmScreen},
    WlxCapture,
};

use glam::{vec2, vec3a, Affine2, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{Haptics, InteractionHandler, PointerHit, PointerMode},
        overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    config::def_pw_tokens,
    config_io,
    graphics::{fourcc_to_vk, WlxCommandBuffer, WlxPipeline, WlxPipelineLegacy},
    hid::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    state::{AppSession, AppState},
};

static DRM_FORMATS: once_cell::sync::OnceCell<Vec<DrmFormat>> = once_cell::sync::OnceCell::new();

pub struct ScreenInteractionHandler {
    next_scroll: Instant,
    next_move: Instant,
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
            next_move: Instant::now(),
            mouse_transform: transform,
        }
    }
}

impl InteractionHandler for ScreenInteractionHandler {
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        #[cfg(debug_assertions)]
        log::trace!("Hover: {:?}", hit.uv);
        if self.next_move < Instant::now() {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.hid_provider.mouse_move(pos);
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
            self.next_move = Instant::now()
                + Duration::from_millis(app.session.config.click_freeze_time_ms as u64);
        }

        app.hid_provider.send_button(btn, pressed);

        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.hid_provider.mouse_move(pos);
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32) {
        if self.next_scroll > Instant::now() {
            return;
        }
        let max_millis = if matches!(hit.mode, PointerMode::Right) {
            50.0
        } else {
            100.0
        };

        let millis = (1. - delta.abs()) * max_millis;
        self.next_scroll = Instant::now().add(Duration::from_millis(millis as _));
        app.hid_provider.wheel(if delta < 0. { -1 } else { 1 })
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
}

struct ScreenPipeline {
    view: Arc<ImageView>,
    mouse: Option<Arc<ImageView>>,
    pipeline: Arc<WlxPipeline<WlxPipelineLegacy>>,
    extentf: [f32; 2],
}

impl ScreenPipeline {
    fn new(extent: &[u32; 3], app: &mut AppState) -> ScreenPipeline {
        let texture = app
            .graphics
            .render_texture(extent[0], extent[1], app.graphics.native_format);

        let view = ImageView::new_default(texture).unwrap();

        let shaders = app.graphics.shared_shaders.read().unwrap();

        let pipeline = app.graphics.create_pipeline(
            view.clone(),
            shaders.get("vert_common").unwrap().clone(),
            shaders.get("frag_sprite").unwrap().clone(),
            app.graphics.native_format,
        );

        let extentf = [extent[0] as f32, extent[1] as f32];

        ScreenPipeline {
            view,
            mouse: None,
            pipeline,
            extentf,
        }
    }

    fn ensure_mouse_initialized(&mut self, uploads: &mut WlxCommandBuffer) {
        if self.mouse.is_some() {
            return;
        }

        #[rustfmt::skip]
        let mouse_bytes = [
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
        ];

        let mouse_tex =
            uploads.texture2d(4, 4, vulkano::format::Format::R8G8B8A8_UNORM, &mouse_bytes);
        self.mouse = Some(ImageView::new_default(mouse_tex).unwrap());
    }

    fn render(&mut self, image: Arc<Image>, mouse: Option<&MouseMeta>, app: &mut AppState) {
        let mut cmd = app
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit);
        let view = ImageView::new_default(image).unwrap();
        let set0 = self.pipeline.uniform_sampler(0, view, Filter::Linear);

        let pass = self.pipeline.create_pass(
            self.extentf,
            app.graphics.quad_verts.clone(),
            app.graphics.quad_indices.clone(),
            vec![set0],
        );

        cmd.begin_render_pass(&self.pipeline);
        cmd.run_ref(&pass);

        if let (Some(mouse), Some(mouse_view)) = (mouse, self.mouse.clone()) {
            let vertex_buffer = app.graphics.upload_verts(
                self.extentf[0],
                self.extentf[1],
                mouse.x * self.extentf[0] - 2.,
                mouse.y * self.extentf[1] - 2.,
                4.0,
                4.0,
            );

            let set0 = self
                .pipeline
                .uniform_sampler(0, mouse_view.clone(), Filter::Linear);

            let pass = self.pipeline.create_pass(
                self.extentf,
                vertex_buffer,
                app.graphics.quad_indices.clone(),
                vec![set0],
            );

            cmd.run_ref(&pass);
        }

        cmd.end_render_pass();

        cmd.build_and_execute_now();
    }
}

pub struct ScreenRenderer {
    name: Arc<str>,
    capture: Box<dyn WlxCapture>,
    pipeline: Option<ScreenPipeline>,
    receiver: Option<Receiver<WlxFrame>>,
    last_view: Option<Arc<ImageView>>,
    extent: [u32; 3],
}

impl ScreenRenderer {
    #[cfg(feature = "wayland")]
    pub fn new_wlr(output: &WlxOutput) -> Option<ScreenRenderer> {
        let client = WlxClient::new()?;
        let capture = WlrDmabufCapture::new(client, output.id);

        Some(ScreenRenderer {
            name: output.name.clone(),
            capture: Box::new(capture),
            pipeline: None,
            receiver: None,
            last_view: None,
            extent: extent_from_res(output.size),
        })
    }

    #[cfg(feature = "wayland")]
    pub fn new_pw(
        output: &WlxOutput,
        token: Option<&str>,
    ) -> Option<(
        ScreenRenderer,
        Option<String>, /* pipewire restore token */
    )> {
        let name = output.name.clone();
        let select_screen_result =
            futures::executor::block_on(pipewire_select_screen(token)).ok()?;

        let capture = PipewireCapture::new(name, select_screen_result.node_id, 60);

        Some((
            ScreenRenderer {
                name: output.name.clone(),
                capture: Box::new(capture),
                pipeline: None,
                receiver: None,
                last_view: None,
                extent: extent_from_res(output.size),
            },
            select_screen_result.restore_token,
        ))
    }

    #[cfg(feature = "x11")]
    pub fn new_xshm(screen: Arc<XshmScreen>) -> Option<ScreenRenderer> {
        let capture = XshmCapture::new(screen.clone());

        Some(ScreenRenderer {
            name: screen.name.clone(),
            capture: Box::new(capture),
            pipeline: None,
            receiver: None,
            last_view: None,
            extent: extent_from_res((screen.monitor.width(), screen.monitor.height())),
        })
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, _app: &mut AppState) {}
    fn render(&mut self, app: &mut AppState) {
        let receiver = self.receiver.get_or_insert_with(|| {
            let allow_dmabuf = &*app.session.config.capture_method != "pw_fallback";

            let drm_formats = DRM_FORMATS.get_or_init({
                let graphics = app.graphics.clone();
                move || {
                    if !allow_dmabuf {
                        log::info!("Using MemFd capture due to pw_fallback");
                        return vec![];
                    }
                    log::warn!("Using DMA-buf capture. If screens are blank for you, switch to SHM using:");
                    log::warn!("echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml");

                    let possible_formats = [
                        DRM_FORMAT_ABGR8888.into(),
                        DRM_FORMAT_XBGR8888.into(),
                        DRM_FORMAT_ARGB8888.into(),
                        DRM_FORMAT_XRGB8888.into(),
                    ];
                    let mut final_formats = vec![];

                    for &f in &possible_formats {
                        let vk_fmt = fourcc_to_vk(f);
                        let Ok(props) = graphics.device.physical_device().format_properties(vk_fmt)
                        else {
                            continue;
                        };
                        final_formats.push(DrmFormat {
                            fourcc: f,
                            modifiers: props
                                .drm_format_modifier_properties
                                .iter()
                                // important bit: only allow single-plane
                                .filter(|m| m.drm_format_modifier_plane_count == 1)
                                .map(|m| m.drm_format_modifier)
                                .collect(),
                        })
                    }
                    final_formats
                }
            });

            let rx = self.capture.init(&drm_formats);
            self.capture.request_new_frame();
            rx
        });

        let mut mouse = None;

        for frame in receiver.try_iter() {
            match frame {
                WlxFrame::Dmabuf(frame) => {
                    if !frame.is_valid() {
                        log::error!("Invalid frame");
                        continue;
                    }
                    if let Some(new) = app.graphics.dmabuf_texture(frame) {
                        let view = ImageView::new_default(new.clone()).unwrap();

                        self.last_view = Some(view);
                    } else {
                        log::error!("{}: Failed to create DMA-buf texture", self.name);
                    }
                }
                WlxFrame::MemFd(frame) => {
                    let mut upload = app
                        .graphics
                        .create_command_buffer(CommandBufferUsage::OneTimeSubmit);

                    let Some(fd) = frame.plane.fd else {
                        log::error!("No fd");
                        continue;
                    };
                    log::debug!("{}: New MemFd frame", self.name);
                    let format = fourcc_to_vk(frame.format.fourcc);

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

                    let image =
                        upload.texture2d(frame.format.width, frame.format.height, format, &data);
                    upload.build_and_execute_now();

                    unsafe { libc::munmap(map as *mut _, len) };

                    self.last_view = Some(ImageView::new_default(image).unwrap());
                    self.capture.request_new_frame();
                }
                WlxFrame::MemPtr(frame) => {
                    log::debug!("{}: New MemPtr frame", self.name);
                    let mut upload = app
                        .graphics
                        .create_command_buffer(CommandBufferUsage::OneTimeSubmit);

                    let format = fourcc_to_vk(frame.format.fourcc);

                    let data = unsafe { slice::from_raw_parts(frame.ptr as *const u8, frame.size) };

                    let image =
                        upload.texture2d(frame.format.width, frame.format.height, format, &data);

                    let mut pipeline = None;
                    if mouse.is_some() {
                        let new_pipeline = self.pipeline.get_or_insert_with(|| {
                            let mut pipeline = ScreenPipeline::new(&self.extent, app);
                            self.last_view = Some(pipeline.view.clone());
                            pipeline.ensure_mouse_initialized(&mut upload);
                            pipeline
                        });
                        pipeline = Some(new_pipeline);
                    }

                    upload.build_and_execute_now();

                    if let Some(pipeline) = pipeline {
                        pipeline.render(image, mouse.as_ref(), app);
                    } else {
                        let view = ImageView::new_default(image).unwrap();
                        self.last_view = Some(view);
                    }
                    self.capture.request_new_frame();
                }
                WlxFrame::Mouse(m) => {
                    mouse = Some(m);
                }
            };
        }
    }
    fn pause(&mut self, _app: &mut AppState) {
        self.capture.pause();
    }
    fn resume(&mut self, _app: &mut AppState) {
        self.capture.resume();
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        self.last_view.clone()
    }
    fn extent(&self) -> [u32; 3] {
        self.extent.clone()
    }
}

#[cfg(feature = "wayland")]
fn try_create_screen<O>(
    wl: &WlxClient,
    id: u32,
    pw_token_store: &mut HashMap<String, String>,
    session: &AppSession,
) -> Option<OverlayData<O>>
where
    O: Default,
{
    let output = &wl.outputs.get(id).unwrap();
    log::info!(
        "{}: Res {}x{} Size {:?} Pos {:?}",
        output.name,
        output.size.0,
        output.size.1,
        output.logical_size,
        output.logical_pos,
    );

    let size = (output.size.0, output.size.1);
    let mut capture: Option<ScreenRenderer> = None;

    if &*session.config.capture_method == "auto" && wl.maybe_wlr_dmabuf_mgr.is_some() {
        log::info!("{}: Using Wlr DMA-Buf", &output.name);
        capture = ScreenRenderer::new_wlr(output);
    }

    if capture.is_none() {
        log::info!("{}: Using Pipewire capture", &output.name);

        let display_name = output.name.deref();

        // Find existing token by display
        let token = pw_token_store.get(display_name).map(|s| s.as_str());

        if let Some(t) = token {
            log::info!(
                "Found existing Pipewire token for display {}: {}",
                display_name,
                t
            );
        }

        if let Some((renderer, restore_token)) = ScreenRenderer::new_pw(output, token) {
            capture = Some(renderer);

            if let Some(token) = restore_token {
                if pw_token_store
                    .insert(String::from(display_name), token.clone())
                    .is_none()
                {
                    log::info!("Adding Pipewire token {}", token);
                }
            }
        }
    }
    if let Some(capture) = capture {
        let backend = Box::new(SplitOverlayBackend {
            renderer: Box::new(capture),
            interaction: Box::new(ScreenInteractionHandler::new(
                vec2(output.logical_pos.0 as f32, output.logical_pos.1 as f32),
                vec2(output.logical_size.0 as f32, output.logical_size.1 as f32),
                output.transform,
            )),
        });

        let axis = Vec3::new(0., 0., 1.);

        let angle = if session.config.upright_screen_fix {
            match output.transform {
                Transform::_90 | Transform::Flipped90 => PI / 2.,
                Transform::_180 | Transform::Flipped180 => PI,
                Transform::_270 | Transform::Flipped270 => -PI / 2.,
                _ => 0.,
            }
        } else {
            0.
        };

        let center = Vec2 { x: 0.5, y: 0.5 };
        let interaction_transform = match output.transform {
            Transform::_90 | Transform::Flipped90 => Affine2::from_cols(
                Vec2::NEG_Y * (output.size.0 as f32 / output.size.1 as f32),
                Vec2::NEG_X,
                center,
            ),
            Transform::_180 | Transform::Flipped180 => Affine2::from_cols(
                Vec2::NEG_X,
                Vec2::NEG_Y * (-output.size.0 as f32 / output.size.1 as f32),
                center,
            ),
            Transform::_270 | Transform::Flipped270 => Affine2::from_cols(
                Vec2::Y * (output.size.0 as f32 / output.size.1 as f32),
                Vec2::X,
                center,
            ),
            _ => Affine2::from_cols(
                Vec2::X,
                Vec2::Y * (-output.size.0 as f32 / output.size.1 as f32),
                center,
            ),
        };

        Some(OverlayData {
            state: OverlayState {
                name: output.name.clone(),
                size,
                show_hide: session
                    .config
                    .show_screens
                    .iter()
                    .any(|s| s.as_ref() == output.name.as_ref()),
                grabbable: true,
                recenter: true,
                interactable: true,
                spawn_scale: 1.5 * session.config.desktop_view_scale,
                spawn_point: vec3a(0., 0.5, -1.),
                spawn_rotation: Quat::from_axis_angle(axis, angle),
                interaction_transform,
                ..Default::default()
            },
            backend,
            ..Default::default()
        })
    } else {
        log::warn!("{}: Will not be used", &output.name);
        None
    }
}

#[derive(Deserialize, Serialize, Default)]
pub struct TokenConf {
    #[serde(default = "def_pw_tokens")]
    pub pw_tokens: Vec<(String, String)>,
}

fn get_pw_token_path() -> PathBuf {
    let mut path = config_io::get_conf_d_path();
    path.push("pw_tokens.yaml");
    path
}

pub fn save_pw_token_config(tokens: &HashMap<String, String>) -> Result<(), Box<dyn Error>> {
    let mut conf = TokenConf::default();

    for (name, token) in tokens {
        conf.pw_tokens.push((name.clone(), token.clone()));
    }

    let yaml = serde_yaml::to_string(&conf)?;
    std::fs::write(get_pw_token_path(), yaml)?;

    Ok(())
}

pub fn load_pw_token_config() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let mut map: HashMap<String, String> = HashMap::new();

    let yaml = std::fs::read_to_string(get_pw_token_path())?;
    let conf: TokenConf = serde_yaml::from_str(yaml.as_str())?;

    for (name, token) in conf.pw_tokens {
        map.insert(name, token);
    }

    Ok(map)
}

#[cfg(not(feature = "wayland"))]
pub fn get_screens_wayland<O>(_session: &AppSession) -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    panic!("Wayland support not enabled")
}

#[cfg(feature = "wayland")]
pub fn get_screens_wayland<O>(session: &AppSession) -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    let mut overlays = vec![];
    let wl = WlxClient::new().unwrap();

    // Load existing Pipewire tokens from file
    let mut pw_tokens: HashMap<String, String> = if let Ok(conf) = load_pw_token_config() {
        conf
    } else {
        HashMap::new()
    };

    let pw_tokens_copy = pw_tokens.clone();

    for id in wl.outputs.keys() {
        if let Some(overlay) = try_create_screen(&wl, *id, &mut pw_tokens, session) {
            overlays.push(overlay);
        }
    }

    if pw_tokens_copy != pw_tokens {
        // Token list changed, re-create token config file
        if let Err(err) = save_pw_token_config(&pw_tokens) {
            log::error!("Failed to save Pipewire token config: {}", err);
        }
    }

    let extent = wl.get_desktop_extent();
    (overlays, Vec2::new(extent.0 as f32, extent.1 as f32))
}

#[cfg(not(feature = "x11"))]
pub fn get_screens_x11<O>(session: &AppSession) -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    panic!("X11 support not enabled")
}

#[cfg(feature = "x11")]
pub fn get_screens_x11<O>(session: &AppSession) -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    let mut extent = vec2(0., 0.);

    let overlays = XshmCapture::get_monitors()
        .into_iter()
        .map(|s| {
            log::info!(
                "{}: Res {}x{}, Pos {}x{}",
                s.name,
                s.monitor.width(),
                s.monitor.height(),
                s.monitor.x(),
                s.monitor.y()
            );
            let size = (s.monitor.width(), s.monitor.height());
            let capture: ScreenRenderer = ScreenRenderer::new_xshm(s.clone()).unwrap();

            let backend = Box::new(SplitOverlayBackend {
                renderer: Box::new(capture),
                interaction: Box::new(ScreenInteractionHandler::new(
                    vec2(s.monitor.x() as f32, s.monitor.y() as f32),
                    vec2(s.monitor.width() as f32, s.monitor.height() as f32),
                    Transform::Normal,
                )),
            });

            let interaction_transform = Affine2::from_translation(Vec2 { x: 0.5, y: 0.5 })
                * Affine2::from_scale(Vec2 {
                    x: 1.,
                    y: -size.0 as f32 / size.1 as f32,
                });

            extent.x = extent.x.max((s.monitor.x() + s.monitor.width()) as f32);
            extent.y = extent.y.max((s.monitor.y() + s.monitor.height()) as f32);
            OverlayData {
                state: OverlayState {
                    name: s.name.clone(),
                    size,
                    show_hide: session
                        .config
                        .show_screens
                        .iter()
                        .any(|x| x.as_ref() == s.name.as_ref()),
                    grabbable: true,
                    recenter: true,
                    interactable: true,
                    spawn_scale: 1.5 * session.config.desktop_view_scale,
                    spawn_point: vec3a(0., 0.5, -1.),
                    spawn_rotation: Quat::IDENTITY,
                    interaction_transform,
                    ..Default::default()
                },
                backend,
                ..Default::default()
            }
        })
        .collect();

    (overlays, extent)
}

fn extent_from_res(res: (i32, i32)) -> [u32; 3] {
    // screens above a certain resolution will have severe aliasing

    // TODO make dynamic. maybe don't go above HMD resolution?
    let w = res.0.min(1920) as u32;
    let h = (res.1 as f32 / res.0 as f32 * w as f32) as u32;
    [w, h, 1]
}
