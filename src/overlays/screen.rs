use log::{info, warn};
use std::{
    f32::consts::PI,
    path::Path,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};
use vulkano::{
    command_buffer::CommandBufferUsage,
    format::Format,
    image::{view::ImageView, ImageAccess, ImageLayout, ImageViewAbstract, ImmutableImage},
    Handle, VulkanObject,
};
use wlx_capture::{
    frame::WlxFrame,
    pipewire::{pipewire_select_screen, PipewireCapture},
    wayland::{Transform, WlxClient, WlxOutput},
    wlr::WlrDmabufCapture,
    WlxCapture,
};

use glam::{vec2, Affine2, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{InteractionHandler, PointerHit, PointerMode},
        overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    input::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    state::{AppSession, AppState},
};

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
        if self.next_move < Instant::now() {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.input.mouse_move(pos);
        }
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.input.mouse_move(pos);

        let btn = match hit.mode {
            PointerMode::Right => MOUSE_RIGHT,
            PointerMode::Middle => MOUSE_MIDDLE,
            _ => MOUSE_LEFT,
        };

        if pressed {
            self.next_move = Instant::now() + Duration::from_millis(300);
        }

        app.input.send_button(btn, pressed);
    }
    fn on_scroll(&mut self, app: &mut AppState, _hit: &PointerHit, delta: f32) {
        let millis = (1. - delta.abs()) * delta;
        if let Some(next_scroll) = Instant::now().checked_add(Duration::from_millis(millis as _)) {
            self.next_scroll = next_scroll;
        }
        app.input.wheel(if delta < 0. { -1 } else { 1 })
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
}

pub struct ScreenRenderer {
    capture: Box<dyn WlxCapture>,
    resolution: (i32, i32),
    receiver: Option<Receiver<WlxFrame>>,
    view: Option<Arc<dyn ImageViewAbstract>>,
}

impl ScreenRenderer {
    pub fn new_wlr(output: &WlxOutput) -> Option<ScreenRenderer> {
        let Some(client) = WlxClient::new() else {
            return None;
        };
        let Some(capture) = WlrDmabufCapture::new(client, output.id) else {
            return None;
        };
        Some(ScreenRenderer {
            capture: Box::new(capture),
            resolution: output.size,
            receiver: None,
            view: None,
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
            capture: Box::new(capture),
            resolution: output.size,
            receiver: None,
            view: None,
        })
    }

    pub fn new_xshm() -> ScreenRenderer {
        todo!()
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, _app: &mut AppState) {
        self.receiver = Some(self.capture.init());
    }
    fn render(&mut self, app: &mut AppState) {
        let Some(receiver) = self.receiver.as_mut() else {
            log::error!("No receiver");
            return;
        };

        for frame in receiver.try_iter() {
            match frame {
                WlxFrame::Dmabuf(frame) => {
                    if let Ok(new) = app.graphics.dmabuf_texture(frame) {
                        if let Some(current) = self.view.as_ref() {
                            if current.image().inner().image.handle().as_raw()
                                == new.inner().image.handle().as_raw()
                            {
                                return;
                            }
                        }
                        app.graphics
                            .transition_layout(
                                new.inner().image.clone(),
                                ImageLayout::Undefined,
                                ImageLayout::TransferSrcOptimal,
                            )
                            .wait(None)
                            .unwrap();
                        self.view = Some(ImageView::new_default(new).unwrap());
                    }
                }
                WlxFrame::MemFd(frame) => {
                    todo!()
                }
                WlxFrame::MemPtr(frame) => {
                    todo!()
                }
                _ => {}
            };
        }
        self.capture.request_new_frame();
    }
    fn pause(&mut self, _app: &mut AppState) {
        self.capture.pause();
    }
    fn resume(&mut self, _app: &mut AppState) {
        self.capture.resume();
    }
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        self.view.as_ref().and_then(|v| Some(v.clone()))
    }
}

fn try_create_screen<O>(wl: &WlxClient, idx: usize, session: &AppSession) -> Option<OverlayData<O>>
where
    O: Default,
{
    let output = &wl.outputs[idx];
    info!(
        "{}: Res {}x{} Size {:?} Pos {:?}",
        output.name, output.size.0, output.size.1, output.logical_size, output.logical_pos,
    );

    let size = (output.size.0, output.size.1);
    let mut capture: Option<ScreenRenderer> = None;

    if session.capture_method == "auto" && wl.maybe_wlr_dmabuf_mgr.is_some() {
        info!("{}: Using Wlr DMA-Buf", &output.name);
        capture = ScreenRenderer::new_wlr(output);
    }

    if capture.is_none() {
        info!("{}: Using Pipewire capture", &output.name);
        let file_name = format!("{}.token", &output.name);
        let full_path = Path::new(&session.config_path).join(file_name);
        let token = std::fs::read_to_string(full_path).ok();

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

        Some(OverlayData {
            state: OverlayState {
                name: output.name.clone(),
                size,
                want_visible: idx == 0,
                show_hide: true,
                grabbable: true,
                spawn_rotation: Quat::from_axis_angle(axis, angle),
                ..Default::default()
            },
            backend,
            ..Default::default()
        })
    } else {
        warn!("{}: Will not be used", &output.name);
        None
    }
}

pub fn get_screens_wayland<O>(session: &AppSession) -> Vec<OverlayData<O>>
where
    O: Default,
{
    let mut overlays = vec![];
    let wl = WlxClient::new().unwrap();

    for idx in 0..wl.outputs.len() {
        if let Some(overlay) = try_create_screen(&wl, idx, &session) {
            overlays.push(overlay);
        }
    }
    overlays
}

pub fn get_screens_x11<O>() -> Vec<OverlayData<O>>
where
    O: Default,
{
    todo!()
}
