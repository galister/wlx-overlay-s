use core::slice;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    error::Error,
    f32::consts::PI,
    ops::Deref,
    path::PathBuf,
    ptr,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};
use wlx_capture::{
    frame::{
        DrmFormat, WlxFrame, DRM_FORMAT_ABGR8888, DRM_FORMAT_ARGB8888, DRM_FORMAT_XBGR8888,
        DRM_FORMAT_XRGB8888,
    },
    pipewire::{pipewire_select_screen, PipewireCapture},
    wayland::{wayland_client::protocol::wl_output::Transform, WlxClient, WlxOutput},
    wlr_dmabuf::WlrDmabufCapture,
    WlxCapture,
};

use glam::{vec2, vec3a, Affine2, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{InteractionHandler, PointerHit, PointerMode},
        overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    config::def_pw_tokens,
    config_io,
    graphics::fourcc_to_vk,
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
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) {
        #[cfg(debug_assertions)]
        log::trace!("Hover: {:?}", hit.uv);
        if self.next_move < Instant::now() {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.hid_provider.mouse_move(pos);
        }
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
    fn on_scroll(&mut self, app: &mut AppState, _hit: &PointerHit, delta: f32) {
        let millis = (1. - delta.abs()) * delta;
        if let Some(next_scroll) = Instant::now().checked_add(Duration::from_millis(millis as _)) {
            self.next_scroll = next_scroll;
        }
        app.hid_provider.wheel(if delta < 0. { -1 } else { 1 })
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
}

pub struct ScreenRenderer {
    name: Arc<str>,
    capture: Box<dyn WlxCapture>,
    receiver: Option<Receiver<WlxFrame>>,
    last_view: Option<Arc<ImageView>>,
    extent: [u32; 3],
}

impl ScreenRenderer {
    pub fn new_wlr(output: &WlxOutput) -> Option<ScreenRenderer> {
        let Some(client) = WlxClient::new() else {
            return None;
        };
        let capture = WlrDmabufCapture::new(client, output.id);

        Some(ScreenRenderer {
            name: output.name.clone(),
            capture: Box::new(capture),
            receiver: None,
            last_view: None,
            extent: extent_from_res(output.size),
        })
    }

    pub fn new_pw(
        output: &WlxOutput,
        token: Option<&str>,
        _fallback: bool,
    ) -> Option<ScreenRenderer> {
        let name = output.name.clone();
        let node_id = futures::executor::block_on(pipewire_select_screen(token)).ok()?;

        let capture = PipewireCapture::new(name, node_id, 60);

        Some(ScreenRenderer {
            name: output.name.clone(),
            capture: Box::new(capture),
            receiver: None,
            last_view: None,
            extent: extent_from_res(output.size),
        })
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, app: &mut AppState) {
        let images = app.graphics.shared_images.read().unwrap();
        self.last_view = Some(images.get("fallback").unwrap().clone());
    }
    fn render(&mut self, app: &mut AppState) {
        let receiver = self.receiver.get_or_insert_with(|| {
            let _drm_formats = DRM_FORMATS.get_or_init({
                let graphics = app.graphics.clone();
                move || {
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
                                .map(|m| m.drm_format_modifier)
                                .collect(),
                        })
                    }
                    final_formats
                }
            });

            let rx = self.capture.init(&[]); // TODO: use drm_formats
            self.capture.request_new_frame();
            rx
        });

        for frame in receiver.try_iter() {
            match frame {
                WlxFrame::Dmabuf(frame) => {
                    if !frame.is_valid() {
                        log::error!("Invalid frame");
                        continue;
                    }
                    if let Some(new) = app.graphics.dmabuf_texture(frame) {
                        log::info!("{}: New DMA-buf frame", self.name);

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

                    let len = frame.format.width as usize * frame.format.height as usize;
                    let data = unsafe { slice::from_raw_parts(frame.ptr as *const u8, len) };

                    let image =
                        upload.texture2d(frame.format.width, frame.format.height, format, &data);
                    upload.build_and_execute_now();

                    self.last_view = Some(ImageView::new_default(image).unwrap());
                    self.capture.request_new_frame();
                }
                _ => {}
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

    if session.capture_method == "auto" && wl.maybe_wlr_dmabuf_mgr.is_some() {
        log::info!("{}: Using Wlr DMA-Buf", &output.name);
        capture = ScreenRenderer::new_wlr(output);
    }

    if capture.is_none() {
        log::info!("{}: Using Pipewire capture", &output.name);

        let display_name = output.name.deref();

        // Find existing token by display
        let token = pw_token_store.get(display_name).map(|s| s.as_str());

        if let Some(t) = token {
            println!(
                "Found existing Pipewire token for display {}: {}",
                display_name, t
            );
        }

        capture = ScreenRenderer::new_pw(
            output,
            token.as_deref(),
            session.capture_method == "pw_fallback",
        );
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

        let angle = match output.transform {
            Transform::_90 | Transform::Flipped90 => PI / 2.,
            Transform::_180 | Transform::Flipped180 => PI,
            Transform::_270 | Transform::Flipped270 => -PI / 2.,
            _ => 0.,
        };

        let interaction_transform = if output.size.0 >= output.size.1 {
            Affine2::from_translation(Vec2 { x: 0.5, y: 0.5 })
                * Affine2::from_scale(Vec2 {
                    x: 1.,
                    y: -output.size.0 as f32 / output.size.1 as f32,
                })
        } else {
            Affine2::from_translation(Vec2 { x: 0.5, y: 0.5 })
                * Affine2::from_scale(Vec2 {
                    x: output.size.1 as f32 / output.size.0 as f32,
                    y: -1.,
                })
        };

        Some(OverlayData {
            state: OverlayState {
                name: output.name.clone(),
                size,
                want_visible: session.show_screens.iter().any(|s| s == &*output.name),
                grabbable: true,
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

pub fn get_screens_x11<O>() -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    todo!()
}

fn extent_from_res(res: (i32, i32)) -> [u32; 3] {
    // screens above a certain resolution will have severe aliasing

    // TODO make dynamic. maybe don't go above HMD resolution?
    let w = res.0.min(1920) as u32;
    let h = (res.1 as f32 / res.0 as f32 * w as f32) as u32;
    [w, h, 1]
}
