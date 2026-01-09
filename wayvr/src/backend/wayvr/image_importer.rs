use std::{collections::HashMap, os::fd::AsRawFd, sync::Arc};

use anyhow::Context;
use smithay::{
    backend::allocator::{
        Buffer,
        dmabuf::{Dmabuf, WeakDmabuf},
    },
    wayland::{
        shm::{BufferData, shm_format_to_fourcc},
        single_pixel_buffer::SinglePixelBufferUserData,
    },
};
use vulkano::{format::Format, image::view::ImageView};
use wgui::gfx::WGfx;
use wlx_capture::frame::{DmabufFrame, FrameFormat, Transform};

use crate::graphics::dmabuf::{WGfxDmabuf, fourcc_to_vk};

pub struct ImageImporter {
    gfx: Arc<WGfx>,
    dmabufs: HashMap<WeakDmabuf, Arc<ImageView>>,
}

impl ImageImporter {
    pub fn new(gfx: Arc<WGfx>) -> Self {
        Self {
            gfx,
            dmabufs: HashMap::new(),
        }
    }

    pub fn import_spb(
        &mut self,
        spb: &SinglePixelBufferUserData,
    ) -> anyhow::Result<Arc<ImageView>> {
        let mut cmd_buf = self.gfx.create_xfer_command_buffer(
            vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
        )?;

        let rgba = spb.rgba8888();
        let image = cmd_buf.upload_image(1, 1, Format::R8G8B8A8_UNORM, &rgba)?;

        cmd_buf.build_and_execute_now()?; //TODO: async

        let image_view = ImageView::new_default(image)?;
        Ok(image_view)
    }

    pub fn import_shm(
        &mut self,
        data: *const u8,
        size: usize,
        bd: BufferData,
    ) -> anyhow::Result<Arc<ImageView>> {
        let mut cmd_buf = self.gfx.create_xfer_command_buffer(
            vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
        )?;

        let fourcc = shm_format_to_fourcc(bd.format)
            .with_context(|| format!("Could not convert {:?} to fourcc", bd.format))?;

        let format = fourcc_to_vk(fourcc)
            .with_context(|| format!("Could not convert {fourcc} to vkFormat"))?;

        let data = unsafe { std::slice::from_raw_parts(data, size) };
        let image = cmd_buf.upload_image(bd.width as _, bd.height as _, format, data)?;

        cmd_buf.build_and_execute_now()?; //TODO: async

        let image_view = ImageView::new_default(image)?;
        Ok(image_view)
    }

    pub fn get_or_import_dmabuf(&mut self, dmabuf: Dmabuf) -> anyhow::Result<Arc<ImageView>> {
        let mut frame = DmabufFrame {
            format: FrameFormat {
                width: dmabuf.width(),
                height: dmabuf.height(),
                drm_format: dmabuf.format(),
                transform: Transform::Undefined,
            },
            num_planes: dmabuf.num_planes(),
            planes: Default::default(),
            mouse: None,
        };

        for (i, handle) in dmabuf.handles().enumerate() {
            // even if the original OwnedFd is dropped, the vkImage will hold reference on the DMA-buf
            frame.planes[i].fd = Some(handle.as_raw_fd());
        }

        for (i, offset) in dmabuf.offsets().enumerate() {
            frame.planes[i].offset = offset;
        }

        for (i, stride) in dmabuf.strides().enumerate() {
            frame.planes[i].stride = stride as _;
        }

        let image = self.gfx.dmabuf_texture(frame)?;
        let image_view = ImageView::new_default(image)?;
        self.dmabufs.insert(dmabuf.weak(), image_view.clone());

        Ok(image_view)
    }

    pub fn cleanup(&mut self) {
        self.dmabufs.retain(|k, _| !k.is_gone());
    }
}
