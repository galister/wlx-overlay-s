use core::slice;
use serde::{Deserialize, Serialize};
use std::{
    f32::consts::PI,
    ptr,
    sync::{atomic::AtomicU64, Arc, LazyLock},
    time::Instant,
};
use vulkano::{
    command_buffer::CommandBufferUsage,
    device::Queue,
    format::Format,
    image::{sampler::Filter, view::ImageView, Image},
    pipeline::graphics::color_blend::AttachmentBlend,
};
use wlx_capture::frame as wlx_frame;

use wlx_capture::{
    frame::{FrameFormat, MouseMeta, WlxFrame},
    WlxCapture,
};

#[cfg(feature = "pipewire")]
use {
    crate::config_io,
    std::error::Error,
    std::{path::PathBuf, task},
    wlx_capture::pipewire::PipewireCapture,
    wlx_capture::pipewire::PipewireSelectScreenResult,
};

#[cfg(all(feature = "x11", feature = "pipewire"))]
use wlx_capture::pipewire::PipewireStream;

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
        overlay::{
            FrameMeta, OverlayRenderer, OverlayState, Positioning, ShouldRender,
            SplitOverlayBackend,
        },
    },
    config::{def_pw_tokens, GeneralConfig, PwTokenMap},
    graphics::{fourcc_to_vk, CommandBuffers, WlxGraphics, WlxPipeline, WlxUploadsBuffer},
    hid::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    state::{AppSession, AppState, KeyboardFocus, ScreenMeta},
};

#[cfg(feature = "wayland")]
pub type WlxClientAlias = wlx_capture::wayland::WlxClient;

#[cfg(not(feature = "wayland"))]
pub(crate) type WlxClientAlias = ();

const CURSOR_SIZE: f32 = 16. / 1440.;

static START: LazyLock<Instant> = LazyLock::new(Instant::now);
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
    mouse_transform: Affine2,
}
impl ScreenInteractionHandler {
    fn new(pos: Vec2, size: Vec2, transform: Transform) -> Self {
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

        Self {
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
            set_next_move(u64::from(app.session.config.mouse_move_interval_ms));
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
            set_next_move(u64::from(app.session.config.click_freeze_time_ms));
        }

        app.hid_provider.send_button(btn, pressed);

        if !pressed {
            return;
        }
        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.hid_provider.mouse_move(pos);
    }
    fn on_scroll(&mut self, app: &mut AppState, _hit: &PointerHit, delta_y: f32, delta_x: f32) {
        app.hid_provider
            .wheel((delta_y * 64.) as i32, (delta_x * 64.) as i32);
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
}

#[derive(Clone)]
struct ScreenPipeline {
    mouse: Option<Arc<ImageView>>,
    pipeline: Arc<WlxPipeline>,
    extentf: [f32; 2],
}

impl ScreenPipeline {
    fn new(extent: &[u32; 3], app: &mut AppState) -> anyhow::Result<Self> {
        let Ok(shaders) = app.graphics.shared_shaders.read() else {
            return Err(anyhow::anyhow!("Could not lock shared shaders for reading"));
        };

        let pipeline = app.graphics.create_pipeline(
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_screen").unwrap().clone(), // want panic
            app.graphics.native_format,
            Some(AttachmentBlend::default()),
        )?;

        let extentf = [extent[0] as f32, extent[1] as f32];

        Ok(Self {
            mouse: None,
            pipeline,
            extentf,
        })
    }

    fn ensure_mouse_initialized(&mut self, uploads: &mut WlxUploadsBuffer) -> anyhow::Result<()> {
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
        mouse: Option<MouseMeta>,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<()> {
        let view = ImageView::new_default(image)?;
        let set0 = self
            .pipeline
            .uniform_sampler(0, view, app.graphics.texture_filtering)?;
        let set1 = self.pipeline.uniform_buffer(1, vec![alpha])?;
        let pass = self
            .pipeline
            .create_pass_for_target(tgt.clone(), vec![set0, set1])?;

        let mut cmd = app
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd.begin_rendering(tgt)?;
        cmd.run_ref(&pass)?;

        if let (Some(mouse), Some(mouse_view)) = (mouse, self.mouse.clone()) {
            let size = CURSOR_SIZE * self.extentf[1];
            let half_size = size * 0.5;

            let vertex_buffer = app.graphics.upload_verts(
                self.extentf[0],
                self.extentf[1],
                mouse.x.mul_add(self.extentf[0], -half_size),
                mouse.y.mul_add(self.extentf[1], -half_size),
                size,
                size,
            )?;

            let set0 = self
                .pipeline
                .uniform_sampler(0, mouse_view, Filter::Nearest)?;

            let set1 = self.pipeline.uniform_buffer(1, vec![alpha])?;

            let pass = self.pipeline.create_pass(
                self.extentf,
                vertex_buffer,
                app.graphics.quad_indices.clone(),
                vec![set0, set1],
            )?;

            cmd.run_ref(&pass)?;
        }

        cmd.end_rendering()?;
        buf.push(cmd.build()?);
        Ok(())
    }
}

macro_rules! new_wlx_capture {
    ($capture_queue:expr, $capture:expr) => {
        if $capture_queue.is_none() {
            Box::new(MainThreadWlxCapture::new($capture)) as Box<dyn WlxCapture<_, _>>
        } else {
            Box::new($capture) as Box<dyn WlxCapture<_, _>>
        }
    };
}

pub struct ScreenRenderer {
    name: Arc<str>,
    capture: Box<dyn WlxCapture<WlxCaptureIn, WlxCaptureOut>>,
    pipeline: Option<ScreenPipeline>,
    cur_frame: Option<WlxCaptureOut>,
    meta: Option<FrameMeta>,
}

impl ScreenRenderer {
    pub fn new_raw(
        name: Arc<str>,
        capture: Box<dyn WlxCapture<WlxCaptureIn, WlxCaptureOut>>,
    ) -> Self {
        Self {
            name,
            capture,
            pipeline: None,
            cur_frame: None,
            meta: None,
        }
    }

    #[cfg(feature = "wayland")]
    pub fn new_wlr_dmabuf(output: &WlxOutput, app: &AppState) -> Option<Self> {
        let client = WlxClient::new()?;
        let capture = new_wlx_capture!(
            app.graphics.capture_queue,
            WlrDmabufCapture::new(client, output.id)
        );
        Some(Self::new_raw(output.name.clone(), capture))
    }

    #[cfg(feature = "wayland")]
    pub fn new_wlr_screencopy(output: &WlxOutput, app: &AppState) -> Option<Self> {
        let client = WlxClient::new()?;
        let capture = new_wlx_capture!(
            app.graphics.capture_queue,
            WlrScreencopyCapture::new(client, output.id)
        );
        Some(Self::new_raw(output.name.clone(), capture))
    }

    #[cfg(feature = "wayland")]
    pub fn new_pw(
        output: &WlxOutput,
        token: Option<&str>,
        app: &AppState,
    ) -> anyhow::Result<(Self, Option<String> /* pipewire restore token */)> {
        let name = output.name.clone();
        let embed_mouse = !app.session.config.double_cursor_fix;

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

        let capture = new_wlx_capture!(
            app.graphics.capture_queue,
            PipewireCapture::new(name, node_id)
        );
        Ok((
            Self::new_raw(output.name.clone(), capture),
            select_screen_result.restore_token,
        ))
    }

    #[cfg(feature = "x11")]
    pub fn new_xshm(screen: Arc<XshmScreen>, app: &AppState) -> Self {
        let capture =
            new_wlx_capture!(app.graphics.capture_queue, XshmCapture::new(screen.clone()));
        Self::new_raw(screen.name.clone(), capture)
    }
}

#[derive(Clone)]
pub struct WlxCaptureIn {
    name: Arc<str>,
    graphics: Arc<WlxGraphics>,
    queue: Arc<Queue>,
}

#[derive(Clone)]
pub struct WlxCaptureOut {
    image: Arc<Image>,
    format: FrameFormat,
    mouse: Option<MouseMeta>,
}

fn upload_image(
    me: &WlxCaptureIn,
    width: u32,
    height: u32,
    format: Format,
    data: &[u8],
) -> Option<Arc<Image>> {
    let mut upload = match me
        .graphics
        .create_uploads_command_buffer(me.queue.clone(), CommandBufferUsage::OneTimeSubmit)
    {
        Ok(x) => x,
        Err(e) => {
            log::error!("{}: Could not create vkCommandBuffer: {:?}", me.name, e);
            return None;
        }
    };
    let image = match upload.texture2d_raw(width, height, format, data) {
        Ok(x) => x,
        Err(e) => {
            log::error!("{}: Could not create vkImage: {:?}", me.name, e);
            return None;
        }
    };

    if let Err(e) = upload.build_and_execute_now() {
        log::error!("{}: Could not execute upload: {:?}", me.name, e);
        return None;
    }

    Some(image)
}

fn receive_callback(me: &WlxCaptureIn, frame: wlx_frame::WlxFrame) -> Option<WlxCaptureOut> {
    match frame {
        WlxFrame::Dmabuf(frame) => {
            if !frame.is_valid() {
                log::error!("{}: Invalid frame", me.name);
                return None;
            }
            log::trace!("{}: New DMA-buf frame", me.name);
            let format = frame.format;
            match me.graphics.dmabuf_texture(frame) {
                Ok(image) => Some(WlxCaptureOut {
                    image,
                    format,
                    mouse: None,
                }),
                Err(e) => {
                    log::error!("{}: Failed to create DMA-buf vkImage: {}", me.name, e);
                    None
                }
            }
        }
        WlxFrame::MemFd(frame) => {
            let Some(fd) = frame.plane.fd else {
                log::error!("{}: No fd in MemFd frame", me.name);
                return None;
            };

            let format = match fourcc_to_vk(frame.format.fourcc) {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{}: {}", me.name, e);
                    return None;
                }
            };

            let len = frame.plane.stride as usize * frame.format.height as usize;
            let offset = i64::from(frame.plane.offset);

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

            let image = {
                let maybe_image =
                    upload_image(me, frame.format.width, frame.format.height, format, data);

                unsafe { libc::munmap(map as *mut _, len) };
                maybe_image
            }?;

            Some(WlxCaptureOut {
                image,
                format: frame.format,
                mouse: None,
            })
        }
        WlxFrame::MemPtr(frame) => {
            log::trace!("{}: New MemPtr frame", me.name);

            let format = match fourcc_to_vk(frame.format.fourcc) {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{}: {}", me.name, e);
                    return None;
                }
            };

            let data = unsafe { slice::from_raw_parts(frame.ptr as *const u8, frame.size) };
            let image = upload_image(me, frame.format.width, frame.format.height, format, data)?;

            Some(WlxCaptureOut {
                image,
                format: frame.format,
                mouse: frame.mouse,
            })
        }
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
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

            let dmabuf_formats = if !supports_dmabuf {
                log::info!("Capture method does not support DMA-buf");
                if app.graphics.capture_queue.is_none() {
                    log::warn!("Current GPU does not support multiple queues. Software capture will take place on the main thread. Expect degraded performance.");
                }
                &Vec::new()
            } else if !allow_dmabuf {
                log::info!("Not using DMA-buf capture due to {capture_method}");
                if app.graphics.capture_queue.is_none() {
                    log::warn!("Current GPU does not support multiple queues. Software capture will take place on the main thread. Expect degraded performance.");
                }
                &Vec::new()
            } else {
                log::warn!(
                    "Using DMA-buf capture. If screens are blank for you, switch to SHM using:"
                );
                log::warn!("echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml");

                &app.graphics.drm_formats
            };

            let user_data = WlxCaptureIn {
                name: self.name.clone(),
                graphics: app.graphics.clone(),
                queue: app
                    .graphics
                    .capture_queue
                    .as_ref()
                    .unwrap_or_else(|| &app.graphics.transfer_queue)
                    .clone(),
            };

            self.capture
                .init(dmabuf_formats, user_data, receive_callback);
            self.capture.request_new_frame();
            return Ok(ShouldRender::Unable);
        }

        if let Some(frame) = self.capture.receive() {
            self.meta = Some(FrameMeta {
                extent: extent_from_format(frame.format, &app.session.config),
                transform: affine_from_format(&frame.format),
                format: frame.image.format(),
            });
            self.cur_frame = Some(frame);
        }

        if let (Some(capture), None) = (self.cur_frame.as_ref(), self.pipeline.as_ref()) {
            self.pipeline = Some({
                let mut pipeline = ScreenPipeline::new(&capture.image.extent(), app)?;
                let mut upload = app.graphics.create_uploads_command_buffer(
                    app.graphics.transfer_queue.clone(),
                    CommandBufferUsage::OneTimeSubmit,
                )?;
                pipeline.ensure_mouse_initialized(&mut upload)?;
                upload.build_and_execute_now()?;
                pipeline
            });
        }

        if self.cur_frame.is_some() {
            Ok(ShouldRender::Should)
        } else {
            Ok(ShouldRender::Unable)
        }
    }
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        let Some(capture) = self.cur_frame.take() else {
            return Ok(false);
        };

        // want panic; must be Some if cur_frame is also Some
        self.pipeline.as_mut().unwrap().render(
            capture.image,
            capture.mouse,
            app,
            tgt,
            buf,
            alpha,
        )?;

        self.capture.request_new_frame();
        Ok(true)
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.pause();
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.resume();
        Ok(())
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.meta
    }
}

#[cfg(feature = "wayland")]
#[allow(clippy::useless_let_if_seq)]
pub fn create_screen_renderer_wl(
    output: &WlxOutput,
    has_wlr_dmabuf: bool,
    has_wlr_screencopy: bool,
    pw_token_store: &mut PwTokenMap,
    app: &AppState,
) -> Option<ScreenRenderer> {
    let mut capture: Option<ScreenRenderer> = None;
    if (&*app.session.config.capture_method == "wlr-dmabuf") && has_wlr_dmabuf {
        log::info!("{}: Using Wlr DMA-Buf", &output.name);
        capture = ScreenRenderer::new_wlr_dmabuf(output, app);
    }

    if &*app.session.config.capture_method == "screencopy" && has_wlr_screencopy {
        log::info!("{}: Using Wlr Screencopy Wl-SHM", &output.name);
        capture = ScreenRenderer::new_wlr_screencopy(output, app);
    }

    if capture.is_none() {
        log::info!("{}: Using Pipewire capture", &output.name);

        let display_name = &*output.name;

        // Find existing token by display
        let token = pw_token_store
            .arc_get(display_name)
            .map(std::string::String::as_str);

        if let Some(t) = token {
            log::info!("Found existing Pipewire token for display {display_name}: {t}");
        }

        match ScreenRenderer::new_pw(output, token, app) {
            Ok((renderer, restore_token)) => {
                capture = Some(renderer);

                if let Some(token) = restore_token {
                    if pw_token_store.arc_set(display_name.into(), token.clone()) {
                        log::info!("Adding Pipewire token {token}");
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
        name,
        keyboard_focus: Some(KeyboardFocus::PhysicalScreen),
        grabbable: true,
        recenter: true,
        positioning: Positioning::Anchored,
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

pub struct ScreenCreateData {
    pub screens: Vec<(ScreenMeta, OverlayState, Box<SplitOverlayBackend>)>,
}

#[cfg(not(feature = "wayland"))]
pub fn create_screens_wayland(_wl: &mut WlxClientAlias, _app: &AppState) -> ScreenCreateData {
    ScreenCreateData {
        screens: Vec::default(),
    }
}

#[cfg(feature = "wayland")]
pub fn create_screens_wayland(wl: &mut WlxClientAlias, app: &mut AppState) -> ScreenCreateData {
    let mut screens = vec![];

    // Load existing Pipewire tokens from file
    let mut pw_tokens: PwTokenMap = load_pw_token_config().unwrap_or_default();

    let pw_tokens_copy = pw_tokens.clone();
    let has_wlr_dmabuf = wl.maybe_wlr_dmabuf_mgr.is_some();
    let has_wlr_screencopy = wl.maybe_wlr_screencopy_mgr.is_some();

    for (id, output) in &wl.outputs {
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
            app,
        ) {
            let logical_pos = vec2(output.logical_pos.0 as f32, output.logical_pos.1 as f32);
            let logical_size = vec2(output.logical_size.0 as f32, output.logical_size.1 as f32);
            let transform = output.transform.into();
            let interaction = create_screen_interaction(logical_pos, logical_size, transform);

            let logical_size_landscape = if output.size.0 > output.size.1 {
                output.logical_size
            } else {
                (output.logical_size.1, output.logical_size.0)
            };

            let state = create_screen_state(
                output.name.clone(),
                logical_size_landscape,
                transform,
                &app.session,
            );

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
            log::error!("Failed to save Pipewire token config: {err}");
        }
    }

    let extent = wl.get_desktop_extent();
    let origin = wl.get_desktop_origin();

    app.hid_provider
        .set_desktop_extent(vec2(extent.0 as f32, extent.1 as f32));
    app.hid_provider
        .set_desktop_origin(vec2(origin.0 as f32, origin.1 as f32));

    ScreenCreateData { screens }
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
    use wlx_capture::xshm::xshm_get_monitors;

    // Load existing Pipewire tokens from file
    let mut pw_tokens: PwTokenMap = load_pw_token_config().unwrap_or_default();
    let pw_tokens_copy = pw_tokens.clone();
    let token = pw_tokens.arc_get("x11").map(std::string::String::as_str);
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
            log::info!("Adding Pipewire token {restore_token}");
        }
    }
    if pw_tokens_copy != pw_tokens {
        // Token list changed, re-create token config file
        if let Err(err) = save_pw_token_config(pw_tokens) {
            log::error!("Failed to save Pipewire token config: {err}");
        }
    }

    let monitors = match xshm_get_monitors() {
        Ok(m) => m,
        Err(e) => {
            anyhow::bail!(e.to_string());
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

            let renderer = ScreenRenderer::new_raw(
                m.name.clone(),
                new_wlx_capture!(
                    app.graphics.capture_queue,
                    PipewireCapture::new(m.name.clone(), s.node_id)
                ),
            );

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
    use wlx_capture::xshm::xshm_get_monitors;

    let mut extent = vec2(0., 0.);

    let monitors = match xshm_get_monitors() {
        Ok(m) => m,
        Err(e) => {
            anyhow::bail!(e.to_string());
        }
    };

    let screens = monitors
        .into_iter()
        .map(|s| {
            extent.x = extent.x.max((s.monitor.x() + s.monitor.width()) as f32);
            extent.y = extent.y.max((s.monitor.y() + s.monitor.height()) as f32);

            let size = (s.monitor.width(), s.monitor.height());
            let pos = (s.monitor.x(), s.monitor.y());
            let renderer = ScreenRenderer::new_xshm(s.clone(), app);

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
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[cfg(feature = "wayland")]
impl From<wl_output::Transform> for Transform {
    fn from(t: wl_output::Transform) -> Self {
        match t {
            wl_output::Transform::_90 => Self::_90,
            wl_output::Transform::_180 => Self::_180,
            wl_output::Transform::_270 => Self::_270,
            wl_output::Transform::Flipped => Self::Flipped,
            wl_output::Transform::Flipped90 => Self::Flipped90,
            wl_output::Transform::Flipped180 => Self::Flipped180,
            wl_output::Transform::Flipped270 => Self::Flipped270,
            _ => Self::Normal,
        }
    }
}

fn extent_from_format(fmt: FrameFormat, config: &GeneralConfig) -> [u32; 3] {
    // screens above a certain resolution will have severe aliasing
    let height_limit = if config.screen_render_down {
        u32::from(config.screen_max_height.min(2560))
    } else {
        2560
    };

    let h = fmt.height.min(height_limit);
    let w = (fmt.width as f32 / fmt.height as f32 * h as f32) as u32;
    [w, h, 1]
}

fn affine_from_format(format: &FrameFormat) -> Affine3A {
    const FLIP_X: Vec3 = Vec3 {
        x: -1.0,
        y: 1.0,
        z: 1.0,
    };

    match format.transform {
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
        _ => Affine3A::IDENTITY,
    }
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

    let mut best_dist = best.map_or(i32::MAX, |b| {
        (b.monitor.x() - position.0).abs() + (b.monitor.y() - position.1).abs()
    });
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
#[allow(clippy::fn_params_excessive_bools)]
fn select_pw_screen(
    instructions: &str,
    token: Option<&str>,
    embed_mouse: bool,
    screens_only: bool,
    persist: bool,
    multiple: bool,
) -> Result<PipewireSelectScreenResult, wlx_capture::pipewire::AshpdError> {
    use crate::backend::notifications::DbusNotificationSender;
    use std::time::Duration;
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
                        log::info!("{instructions}");
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

// Used when a separate GPU queue is not available
// In this case, receive_callback needs to run on the main thread
struct MainThreadWlxCapture<T>
where
    T: WlxCapture<(), WlxFrame>,
{
    inner: T,
    data: Option<WlxCaptureIn>,
}

impl<T> MainThreadWlxCapture<T>
where
    T: WlxCapture<(), WlxFrame>,
{
    pub const fn new(inner: T) -> Self {
        Self { inner, data: None }
    }
}

impl<T> WlxCapture<WlxCaptureIn, WlxCaptureOut> for MainThreadWlxCapture<T>
where
    T: WlxCapture<(), WlxFrame>,
{
    fn init(
        &mut self,
        dmabuf_formats: &[wlx_frame::DrmFormat],
        user_data: WlxCaptureIn,
        _: fn(&WlxCaptureIn, WlxFrame) -> Option<WlxCaptureOut>,
    ) {
        self.data = Some(user_data);
        self.inner.init(dmabuf_formats, (), receive_callback_dummy);
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn request_new_frame(&mut self) {
        self.inner.request_new_frame();
    }
    fn pause(&mut self) {
        self.inner.pause();
    }
    fn resume(&mut self) {
        self.inner.resume();
    }
    fn receive(&mut self) -> Option<WlxCaptureOut> {
        self.inner
            .receive()
            .and_then(|frame| receive_callback(self.data.as_ref().unwrap(), frame))
    }
    fn supports_dmbuf(&self) -> bool {
        self.inner.supports_dmbuf()
    }
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::unnecessary_wraps)]
const fn receive_callback_dummy(_: &(), frame: wlx_frame::WlxFrame) -> Option<wlx_frame::WlxFrame> {
    Some(frame)
}
