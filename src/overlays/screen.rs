use std::{
    f32::consts::PI,
    path::Path,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};
use vulkano::{
    buffer::Subbuffer,
    command_buffer::CommandBufferUsage,
    format::Format,
    image::{
        view::ImageView, AttachmentImage, ImageAccess, ImageLayout, ImageViewAbstract, StorageImage,
    },
    sampler::Filter,
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

use glam::{vec2, Affine2, Quat, Vec2, Vec3};

use crate::{
    backend::{
        input::{InteractionHandler, PointerHit, PointerMode},
        overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
    },
    graphics::{Vert2Uv, WlxGraphics, WlxPipeline},
    input::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    shaders::{frag_sprite, vert_common},
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
            self.next_move =
                Instant::now() + Duration::from_millis(app.session.click_freeze_time_ms);
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

struct ScreenPipeline {
    graphics: Arc<WlxGraphics>,
    pipeline: Arc<WlxPipeline>,
    vertex_buffer: Subbuffer<[Vert2Uv]>,
    target_layout: ImageLayout,
    pub view: Arc<ImageView<AttachmentImage>>,
}

impl ScreenPipeline {
    fn new(graphics: Arc<WlxGraphics>, image: &StorageImage) -> Self {
        let pipeline = graphics.create_pipeline(
            vert_common::load(graphics.device.clone()).unwrap(),
            frag_sprite::load(graphics.device.clone()).unwrap(),
            Format::R8G8B8A8_UNORM,
        );

        let dim = image.dimensions().width_height();

        let vertex_buffer =
            graphics.upload_verts(dim[0] as _, dim[1] as _, 0.0, 0.0, dim[0] as _, dim[1] as _);

        let render_texture = graphics.render_texture(dim[0], dim[1], Format::R8G8B8A8_UNORM);

        let view = ImageView::new_default(render_texture).unwrap();

        Self {
            graphics,
            pipeline,
            vertex_buffer,
            view,
            target_layout: ImageLayout::Undefined,
        }
    }

    fn render(&mut self, image: Arc<StorageImage>) {
        if image.inner().image.handle().as_raw()
            == self.view.image().inner().image.handle().as_raw()
        {
            return;
        }

        let mut command_buffer = self
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin(self.view.clone());

        let set0 = self.pipeline.uniform_sampler(
            0,
            ImageView::new_default(image).unwrap(),
            Filter::Linear,
        );

        let dim = self.view.dimensions().width_height();
        let dim = [dim[0] as f32, dim[1] as f32];

        let pass = self.pipeline.create_pass(
            dim,
            self.vertex_buffer.clone(),
            self.graphics.quad_indices.clone(),
            vec![set0],
        );
        command_buffer.run_ref(&pass);

        let image = self.view.image().inner().image.clone();

        if self.target_layout == ImageLayout::TransferSrcOptimal {
            self.graphics
                .transition_layout(
                    image.clone(),
                    ImageLayout::TransferSrcOptimal,
                    ImageLayout::ColorAttachmentOptimal,
                )
                .wait(None)
                .unwrap();
        }

        {
            let mut exec = command_buffer.end_render().build_and_execute();
            exec.flush().unwrap();
            exec.cleanup_finished();
        }

        self.graphics
            .transition_layout(
                image,
                ImageLayout::ColorAttachmentOptimal,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();

        self.target_layout = ImageLayout::TransferSrcOptimal;
    }
}

pub struct ScreenRenderer {
    capture: Box<dyn WlxCapture>,
    receiver: Option<Receiver<WlxFrame>>,
    pipeline: Option<ScreenPipeline>,
    last_frame: Option<Arc<dyn ImageViewAbstract>>,
}

impl ScreenRenderer {
    pub fn new_wlr(output: &WlxOutput) -> Option<ScreenRenderer> {
        let Some(client) = WlxClient::new() else {
            return None;
        };
        let capture = WlrDmabufCapture::new(client, output.id);
        Some(ScreenRenderer {
            capture: Box::new(capture),
            receiver: None,
            pipeline: None,
            last_frame: None,
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
            receiver: None,
            pipeline: None,
            last_frame: None,
        })
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
                        let pipeline = self
                            .pipeline
                            .get_or_insert_with(|| ScreenPipeline::new(app.graphics.clone(), &new));

                        pipeline.render(new);
                        self.last_frame = Some(pipeline.view.clone());
                    }
                }
                WlxFrame::MemFd(_frame) => {
                    todo!()
                }
                WlxFrame::MemPtr(_frame) => {
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
        self.last_frame.take()
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
        capture = ScreenRenderer::new_wlr(output);
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
