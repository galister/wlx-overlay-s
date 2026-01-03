use std::{f32::consts::PI, sync::Arc};

use glam::{Affine3A, Vec3};
use smallvec::{SmallVec, smallvec};
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
    DrmFormat, WlxCapture,
    frame::{self as wlx_frame, FrameFormat, MouseMeta, WlxFrame},
};
use wlx_common::{config::GeneralConfig, overlays::StereoMode};

use crate::{
    graphics::{
        Vert2Uv,
        dmabuf::{WGfxDmabuf, fourcc_to_vk},
        upload_quad_vertices,
    },
    state::AppState,
    windowing::backend::{FrameMeta, RenderResources},
};

const CURSOR_SIZE: f32 = 16. / 1440.;

struct BufPass {
    pass: WGfxPass<Vert2Uv>,
    buf_vert: Subbuffer<[Vert2Uv]>,
}

/// A render pipeline that supports mouse + stereo
pub struct ScreenPipeline {
    mouse: BufPass,
    pass: SmallVec<[BufPass; 2]>,
    pipeline: Arc<WGfxPipeline<Vert2Uv>>,
    buf_alpha: Subbuffer<[f32]>,
    extentf: [f32; 2],
    stereo: StereoMode,
}

impl ScreenPipeline {
    pub fn new(meta: &FrameMeta, app: &mut AppState, stereo: StereoMode) -> anyhow::Result<Self> {
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

        let mut me = Self {
            pass: smallvec![Self::create_pass(
                app,
                pipeline.clone(),
                extentf,
                buf_alpha.clone()
            )?],
            mouse: Self::create_mouse_pass(app, pipeline.clone(), extentf, buf_alpha.clone())?,
            pipeline,
            extentf,
            buf_alpha,
            stereo,
        };
        me.set_stereo(app, stereo)?;
        Ok(me)
    }

    pub fn set_stereo(&mut self, app: &mut AppState, stereo: StereoMode) -> anyhow::Result<()> {
        let depth = if matches!(stereo, StereoMode::None) {
            1
        } else {
            2
        };

        if self.pass.len() < depth {
            self.pass.push(Self::create_pass(
                app,
                self.pipeline.clone(),
                self.extentf,
                self.buf_alpha.clone(),
            )?);
        }

        if self.pass.len() > depth {
            self.pass.pop();
        }

        for (eye, current) in self.pass.iter_mut().enumerate() {
            let verts = stereo_mode_to_verts(stereo, eye);
            current.buf_vert.write()?.copy_from_slice(&verts);
        }
        Ok(())
    }

    pub fn get_depth(&self) -> u32 {
        self.pass.len() as _
    }

    pub fn set_extent(&mut self, app: &mut AppState, extentf: [f32; 2]) -> anyhow::Result<()> {
        for (eye, pass) in self.pass.iter_mut().enumerate() {
            *pass = Self::create_pass(app, self.pipeline.clone(), extentf, self.buf_alpha.clone())?;
            let verts = stereo_mode_to_verts(self.stereo, eye);
            pass.buf_vert.write()?.copy_from_slice(&verts);
        }

        self.mouse =
            Self::create_mouse_pass(app, self.pipeline.clone(), extentf, self.buf_alpha.clone())?;
        Ok(())
    }

    fn create_pass(
        app: &mut AppState,
        pipeline: Arc<WGfxPipeline<Vert2Uv>>,
        extentf: [f32; 2],
        buf_alpha: Subbuffer<[f32]>,
    ) -> anyhow::Result<BufPass> {
        let set0 = pipeline.uniform_sampler(
            0,
            app.gfx_extras.fallback_image.clone(),
            app.gfx.texture_filter,
        )?;
        let set1 = pipeline.buffer(1, buf_alpha)?;
        let buf_vert = app
            .gfx
            .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::VERTEX_BUFFER, 4)?;

        let pass = pipeline.create_pass(
            extentf,
            buf_vert.clone(),
            0..4,
            0..1,
            vec![set0, set1],
            &Default::default(),
        )?;

        Ok(BufPass { pass, buf_vert })
    }

    fn create_mouse_pass(
        app: &mut AppState,
        pipeline: Arc<WGfxPipeline<Vert2Uv>>,
        extentf: [f32; 2],
        buf_alpha: Subbuffer<[f32]>,
    ) -> anyhow::Result<BufPass> {
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
        Ok(BufPass { pass, buf_vert })
    }

    pub fn render(
        &mut self,
        image: Arc<ImageView>,
        mouse: Option<&MouseMeta>,
        app: &mut AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        self.buf_alpha.write()?[0] = rdr.alpha;

        for (eye, cmd_buf) in rdr.cmd_bufs.iter_mut().enumerate() {
            let current = &mut self.pass[eye];

            current
                .pass
                .update_sampler(0, image.clone(), app.gfx.texture_filter)?;

            cmd_buf.run_ref(&current.pass)?;

            if let Some(mouse) = mouse.as_ref() {
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

                cmd_buf.run_ref(&self.mouse.pass)?;
            }
        }

        Ok(())
    }

    pub fn get_alpha_buf(&self) -> Subbuffer<[f32]> {
        self.buf_alpha.clone()
    }
}

fn stereo_mode_to_verts(stereo: StereoMode, array_index: usize) -> [Vert2Uv; 4] {
    let eye = match stereo {
        StereoMode::RightLeft | StereoMode::BottomTop => (1 - array_index) as f32,
        _ => array_index as f32,
    };

    match stereo {
        StereoMode::None => [
            Vert2Uv {
                in_pos: [0., 0.],
                in_uv: [0., 0.],
            },
            Vert2Uv {
                in_pos: [1., 0.],
                in_uv: [1., 0.],
            },
            Vert2Uv {
                in_pos: [0., 1.],
                in_uv: [0., 1.],
            },
            Vert2Uv {
                in_pos: [1., 1.],
                in_uv: [1., 1.],
            },
        ],
        StereoMode::LeftRight | StereoMode::RightLeft => [
            Vert2Uv {
                in_pos: [0., 0.],
                in_uv: [eye * 0.5, 0.],
            },
            Vert2Uv {
                in_pos: [1., 0.],
                in_uv: [0.5 + eye * 0.5, 0.],
            },
            Vert2Uv {
                in_pos: [0., 1.],
                in_uv: [eye * 0.5, 1.],
            },
            Vert2Uv {
                in_pos: [1., 1.],
                in_uv: [0.5 + eye * 0.5, 1.],
            },
        ],
        StereoMode::TopBottom | StereoMode::BottomTop => [
            Vert2Uv {
                in_pos: [0., 0.],
                in_uv: [0., eye * 0.5],
            },
            Vert2Uv {
                in_pos: [1., 0.],
                in_uv: [1., eye * 0.5],
            },
            Vert2Uv {
                in_pos: [0., 1.],
                in_uv: [0., 0.5 + eye * 0.5],
            },
            Vert2Uv {
                in_pos: [1., 1.],
                in_uv: [1., 0.5 + eye * 0.5],
            },
        ],
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
pub(super) struct WlxCaptureOut {
    pub(super) image: Arc<ImageView>,
    pub(super) format: FrameFormat,
    pub(super) mouse: Option<MouseMeta>,
}

impl WlxCaptureOut {
    pub(super) fn get_frame_meta(&self, config: &GeneralConfig) -> FrameMeta {
        FrameMeta {
            clear: WGfxClearMode::DontCare,
            extent: extent_from_format(self.format, config),
            transform: affine_from_format(&self.format),
            format: self.image.format(),
        }
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
                    image: ImageView::new_default(image).ok()?,
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

            let format = match fourcc_to_vk(frame.format.drm_format.code) {
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
                image: ImageView::new_default(image).ok()?,
                format: frame.format,
                mouse: None,
            })
        }
        WlxFrame::MemPtr(frame) => {
            log::trace!("{}: New MemPtr frame", me.name);

            let format = match fourcc_to_vk(frame.format.drm_format.code) {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{}: {}", me.name, e);
                    return None;
                }
            };

            let data = unsafe { std::slice::from_raw_parts(frame.ptr as *const u8, frame.size) };
            let image = upload_image(me, frame.format.width, frame.format.height, format, data)?;

            Some(WlxCaptureOut {
                image: ImageView::new_default(image).ok()?,
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
