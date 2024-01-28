use core::slice;
use std::{
    f32::consts::PI,
    path::Path,
    ptr,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
    usize,
};
use vulkano::{
    buffer::Subbuffer,
    command_buffer::CommandBufferUsage,
    format::Format,
    image::{sampler::Filter, view::ImageView, Image, ImageLayout},
    sync::GpuFuture,
    Handle, VulkanObject,
};
use wlx_capture::{
    frame::WlxFrame,
    pipewire::{pipewire_select_screen, PipewireCapture},
    wayland::{Transform, WlxClient, WlxOutput},
    wlr::WlrDmabufCapture,
    WlxCapture,
};

use glam::{vec2, vec3a, Affine2, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{InteractionHandler, PointerHit, PointerMode},
        overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    graphics::{fourcc_to_vk, Vert2Uv, WlxGraphics, WlxPipeline},
    hid::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
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
        #[cfg(debug_assertions)]
        log::trace!("Hover: {:?}", hit.uv);
        if self.next_move < Instant::now() {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.hid_provider.mouse_move(pos);
        }
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.hid_provider.mouse_move(pos);

        let btn = match hit.mode {
            PointerMode::Right => MOUSE_RIGHT,
            PointerMode::Middle => MOUSE_MIDDLE,
            _ => MOUSE_LEFT,
        };

        if pressed {
            self.next_move =
                Instant::now() + Duration::from_millis(app.session.click_freeze_time_ms);
        }

        app.hid_provider.send_button(btn, pressed);
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

struct ScreenPipeline {
    graphics: Arc<WlxGraphics>,
    pipeline: Arc<WlxPipeline>,
    vertex_buffer: Subbuffer<[Vert2Uv]>,
}

impl ScreenPipeline {
    fn new(graphics: Arc<WlxGraphics>, image: &Image, extent: &[u32; 3]) -> Self {
        let dim = image.extent();

        let vertex_buffer =
            graphics.upload_verts(dim[0] as _, dim[1] as _, 0.0, 0.0, dim[0] as _, dim[1] as _);

        let render_texture = graphics.render_texture(extent[0], extent[1], Format::R8G8B8A8_UNORM);

        let view = ImageView::new_default(render_texture).unwrap();

        let pipeline = {
            let shaders = graphics.shared_shaders.read().unwrap();

            graphics.create_pipeline(
                view,
                shaders.get("vert_common").unwrap().clone(),
                shaders.get("frag_sprite").unwrap().clone(),
                Format::R8G8B8A8_UNORM,
                //            ImageLayout::TransferSrcOptimal,
                //            ImageLayout::TransferSrcOptimal,
            )
        };

        Self {
            graphics,
            pipeline,
            vertex_buffer,
        }
    }

    fn render(&mut self, image: Arc<Image>) {
        if image.handle().as_raw() == self.pipeline.view.image().handle().as_raw() {
            return;
        }

        self.graphics
            .transition_layout(
                self.pipeline.view.image().clone(),
                ImageLayout::TransferSrcOptimal,
                ImageLayout::General,
            )
            .wait(None)
            .unwrap();

        let mut command_buffer = self
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit);
        command_buffer.begin_render_pass(&self.pipeline);

        let set0 = self.pipeline.uniform_sampler(
            0,
            ImageView::new_default(image).unwrap(),
            Filter::Linear,
        );

        let dim = self.pipeline.view.image().extent();
        let dim = [dim[0] as f32, dim[1] as f32];

        let pass = self.pipeline.create_pass(
            dim,
            self.vertex_buffer.clone(),
            self.graphics.quad_indices.clone(),
            vec![set0],
        );
        command_buffer.run_ref(&pass);
        command_buffer.end_render_pass();

        {
            let mut exec = command_buffer.build_and_execute();
            exec.flush().unwrap();
            exec.cleanup_finished();
        }

        self.graphics
            .transition_layout(
                self.pipeline.view.image().clone(),
                ImageLayout::General,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();
    }

    pub(super) fn view(&self) -> Arc<ImageView> {
        self.pipeline.view.clone()
    }
}

pub struct ScreenRenderer {
    name: Arc<str>,
    capture: Box<dyn WlxCapture>,
    receiver: Option<Receiver<WlxFrame>>,
    pipeline: Option<ScreenPipeline>,
    last_image: Option<Arc<ImageView>>,
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
            pipeline: None,
            last_image: None,
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
            pipeline: None,
            last_image: None,
            extent: extent_from_res(output.size),
        })
    }
}

impl OverlayRenderer for ScreenRenderer {
    fn init(&mut self, app: &mut AppState) {
        let images = app.graphics.shared_images.read().unwrap();
        self.last_image = Some(images.get("fallback").unwrap().clone());
    }
    fn render(&mut self, app: &mut AppState) {
        let receiver = self.receiver.get_or_insert_with(|| self.capture.init());

        for frame in receiver.try_iter() {
            match frame {
                WlxFrame::Dmabuf(frame) => {
                    if !frame.is_valid() {
                        log::error!("Invalid frame");
                        continue;
                    }
                    if let Some(new) = app.graphics.dmabuf_texture(frame) {
                        let pipeline = self.pipeline.get_or_insert_with(|| {
                            ScreenPipeline::new(app.graphics.clone(), &new, &self.extent)
                        });
                        log::debug!("{}: New DMA-buf frame", self.name);
                        pipeline.render(new);
                        self.last_image = Some(pipeline.view());
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

                    self.last_image = Some(ImageView::new_default(image).unwrap());
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

                    self.last_image = Some(ImageView::new_default(image).unwrap());
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
    fn view(&mut self) -> Option<Arc<ImageView>> {
        self.last_image.take()
    }
    fn extent(&self) -> [u32; 3] {
        self.extent.clone()
    }
}

fn try_create_screen<O>(wl: &WlxClient, id: u32, session: &AppSession) -> Option<OverlayData<O>>
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
        //capture = ScreenRenderer::new_wlr(output);
    }

    if capture.is_none() {
        log::info!("{}: Using Pipewire capture", &output.name);
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
                show_hide: true,
                grabbable: true,
                spawn_scale: 1.5,
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

pub fn get_screens_wayland<O>(session: &AppSession) -> (Vec<OverlayData<O>>, Vec2)
where
    O: Default,
{
    let mut overlays = vec![];
    let wl = WlxClient::new().unwrap();

    for id in wl.outputs.keys() {
        if let Some(overlay) = try_create_screen(&wl, *id, &session) {
            overlays.push(overlay);
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
