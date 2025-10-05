use std::{f32::consts::PI, sync::Arc};

use glam::{Affine3A, Vec3};
use smallvec::smallvec;
use vulkano::{
    buffer::{BufferUsage, Subbuffer},
    command_buffer::CommandBufferUsage,
    device::Queue,
    format::Format,
    image::{Image, sampler::Filter, view::ImageView},
    pipeline::graphics::color_blend::AttachmentBlend,
};
use wgui::gfx::{
    WGfx,
    cmd::WGfxClearMode,
    pass::WGfxPass,
    pipeline::{WGfxPipeline, WPipelineCreateInfo},
};
use wlx_capture::{
    WlxCapture,
    frame::{self as wlx_frame, DrmFormat, FrameFormat, MouseMeta, Transform, WlxFrame},
};

use crate::{
    config::GeneralConfig,
    graphics::{
        CommandBuffers, Vert2Uv,
        dmabuf::{WGfxDmabuf, fourcc_to_vk},
        upload_quad_vertices,
    },
    state::AppState,
    windowing::backend::FrameMeta,
};

const CURSOR_SIZE: f32 = 16. / 1440.;

struct MousePass {
    pass: WGfxPass<Vert2Uv>,
    buf_vert: Subbuffer<[Vert2Uv]>,
}

pub(super) struct ScreenPipeline {
    mouse: MousePass,
    pipeline: Arc<WGfxPipeline<Vert2Uv>>,
    pass: WGfxPass<Vert2Uv>,
    buf_alpha: Subbuffer<[f32]>,
    extentf: [f32; 2],
}

impl ScreenPipeline {
    pub(super) fn new(meta: &FrameMeta, app: &mut AppState) -> anyhow::Result<Self> {
        let extentf = [meta.extent[0] as f32, meta.extent[1] as f32];

        let pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_screen").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format)
                .use_blend(AttachmentBlend::default())
                .use_updatable_descriptors(smallvec![0]),
        )?;

        let buf_alpha = app
            .gfx
            .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::UNIFORM_BUFFER, 1)?;

        Ok(Self {
            pass: Self::create_pass(app, pipeline.clone(), extentf, buf_alpha.clone())?,
            mouse: Self::create_mouse_pass(app, pipeline.clone(), extentf, buf_alpha.clone())?,
            pipeline,
            extentf,
            buf_alpha,
        })
    }

    pub fn set_extent(&mut self, app: &mut AppState, extentf: [f32; 2]) -> anyhow::Result<()> {
        self.pass = Self::create_pass(app, self.pipeline.clone(), extentf, self.buf_alpha.clone())?;
        self.mouse =
            Self::create_mouse_pass(app, self.pipeline.clone(), extentf, self.buf_alpha.clone())?;
        Ok(())
    }

    fn create_pass(
        app: &mut AppState,
        pipeline: Arc<WGfxPipeline<Vert2Uv>>,
        extentf: [f32; 2],
        buf_alpha: Subbuffer<[f32]>,
    ) -> anyhow::Result<WGfxPass<Vert2Uv>> {
        let set0 = pipeline.uniform_sampler(
            0,
            app.gfx_extras.fallback_image.clone(),
            app.gfx.texture_filter,
        )?;
        let set1 = pipeline.buffer(1, buf_alpha)?;
        pipeline.create_pass(
            extentf,
            app.gfx_extras.quad_verts.clone(),
            0..4,
            0..1,
            vec![set0, set1],
            &Default::default(),
        )
    }

    fn create_mouse_pass(
        app: &mut AppState,
        pipeline: Arc<WGfxPipeline<Vert2Uv>>,
        extentf: [f32; 2],
        buf_alpha: Subbuffer<[f32]>,
    ) -> anyhow::Result<MousePass> {
        #[rustfmt::skip]
        let mouse_bytes = [
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0xff, 0xff, 0xff, 0xff,  0xff, 0xff, 0xff, 0xff,  0x00, 0x00, 0x00, 0xff,
            0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,  0x00, 0x00, 0x00, 0xff,
        ];

        let mut cmd_xfer = app
            .gfx
            .create_xfer_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        let image =
            cmd_xfer.upload_image(4, 4, vulkano::format::Format::R8G8B8A8_UNORM, &mouse_bytes)?;

        let view = ImageView::new_default(image)?;

        let buf_vert = cmd_xfer
            .graphics
            .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::VERTEX_BUFFER, 4)?;

        let set0 = pipeline.uniform_sampler(0, view, Filter::Nearest)?;
        let set1 = pipeline.buffer(1, buf_alpha)?;
        let pass = pipeline.create_pass(
            extentf,
            buf_vert.clone(),
            0..4,
            0..1,
            vec![set0, set1],
            &Default::default(),
        )?;

        cmd_xfer.build_and_execute_now()?;
        Ok(MousePass { pass, buf_vert })
    }

    pub(super) fn render(
        &mut self,
        capture: &WlxCaptureOut,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<()> {
        let view = ImageView::new_default(capture.image.clone())?;

        self.pass.update_sampler(0, view, app.gfx.texture_filter)?;
        self.buf_alpha.write()?[0] = alpha;

        let mut cmd = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd.begin_rendering(tgt, WGfxClearMode::DontCare)?;
        cmd.run_ref(&self.pass)?;

        if let Some(mouse) = capture.mouse.as_ref() {
            let size = CURSOR_SIZE * self.extentf[1];
            let half_size = size * 0.5;

            upload_quad_vertices(
                &mut self.mouse.buf_vert,
                self.extentf[0],
                self.extentf[1],
                mouse.x.mul_add(self.extentf[0], -half_size),
                mouse.y.mul_add(self.extentf[1], -half_size),
                size,
                size,
            )?;

            cmd.run_ref(&self.mouse.pass)?;
        }

        cmd.end_rendering()?;
        buf.push(cmd.build()?);
        Ok(())
    }
}

#[derive(Clone)]
pub struct WlxCaptureIn {
    name: Arc<str>,
    gfx: Arc<WGfx>,
    queue: Arc<Queue>,
}

impl WlxCaptureIn {
    pub(super) fn new(name: Arc<str>, app: &AppState) -> Self {
        Self {
            name,
            gfx: app.gfx.clone(),
            queue: app
                .gfx_extras
                .queue_capture
                .as_ref()
                .unwrap_or_else(|| &app.gfx.queue_xfer)
                .clone(),
        }
    }
}

#[derive(Clone)]
pub struct WlxCaptureOut {
    image: Arc<Image>,
    format: FrameFormat,
    mouse: Option<MouseMeta>,
}

impl WlxCaptureOut {
    pub(super) fn get_frame_meta(&self, config: &GeneralConfig) -> FrameMeta {
        FrameMeta {
            extent: extent_from_format(self.format, config),
            transform: affine_from_format(&self.format),
            format: self.image.format(),
        }
    }

    pub(super) const fn get_transform(&self) -> Transform {
        self.format.transform
    }
}

fn upload_image(
    me: &WlxCaptureIn,
    width: u32,
    height: u32,
    format: Format,
    data: &[u8],
) -> Option<Arc<Image>> {
    let mut cmd_xfer = match me
        .gfx
        .create_xfer_command_buffer_with_queue(me.queue.clone(), CommandBufferUsage::OneTimeSubmit)
    {
        Ok(x) => x,
        Err(e) => {
            log::error!("{}: Could not create vkCommandBuffer: {:?}", me.name, e);
            return None;
        }
    };
    let image = match cmd_xfer.upload_image(width, height, format, data) {
        Ok(x) => x,
        Err(e) => {
            log::error!("{}: Could not create vkImage: {:?}", me.name, e);
            return None;
        }
    };

    if let Err(e) = cmd_xfer.build_and_execute_now() {
        log::error!("{}: Could not execute upload: {:?}", me.name, e);
        return None;
    }

    Some(image)
}

pub(super) fn receive_callback(me: &WlxCaptureIn, frame: WlxFrame) -> Option<WlxCaptureOut> {
    match frame {
        WlxFrame::Dmabuf(frame) => {
            if !frame.is_valid() {
                log::error!("{}: Invalid frame", me.name);
                return None;
            }
            log::trace!("{}: New DMA-buf frame", me.name);
            let format = frame.format;
            match me.gfx.dmabuf_texture(frame) {
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
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ,
                    libc::MAP_SHARED,
                    fd,
                    offset,
                )
            } as *const u8;

            let data = unsafe { std::slice::from_raw_parts(map, len) };

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

            let data = unsafe { std::slice::from_raw_parts(frame.ptr as *const u8, frame.size) };
            let image = upload_image(me, frame.format.width, frame.format.height, format, data)?;

            Some(WlxCaptureOut {
                image,
                format: frame.format,
                mouse: frame.mouse,
            })
        }
    }
}

// Used when a separate GPU queue is not available
// In this case, receive_callback needs to run on the main thread
pub(super) struct MainThreadWlxCapture<T>
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
        dmabuf_formats: &[DrmFormat],
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
const fn receive_callback_dummy(_: &(), frame: WlxFrame) -> Option<WlxFrame> {
    Some(frame)
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

macro_rules! new_wlx_capture {
    ($capture_queue:expr, $capture:expr) => {
        if $capture_queue.is_none() {
            Box::new(MainThreadWlxCapture::new($capture)) as Box<dyn WlxCapture<_, _>>
        } else {
            Box::new($capture) as Box<dyn WlxCapture<_, _>>
        }
    };
}

pub(super) use new_wlx_capture;
