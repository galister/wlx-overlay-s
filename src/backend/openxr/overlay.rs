use std::sync::Arc;

use super::XrState;
use crate::{
    backend::overlay::OverlayData,
    graphics::{WlxPass, WlxPipeline},
    shaders::{frag_srgb, vert_common},
    state::AppState,
};
use ash::vk::{self};
use openxr as xr;
use vulkano::{
    command_buffer::CommandBufferUsage,
    image::{sampler::Filter, view::ImageView, ImageCreateInfo, ImageUsage},
    render_pass::{Framebuffer, FramebufferCreateInfo},
    Handle,
};

#[derive(Default)]
pub struct OpenXrOverlayData {
    inner: Option<XrOverlayData>,
    pub(super) init: bool,
}

impl OverlayData<OpenXrOverlayData> {
    pub(super) fn initialize(&mut self, xr: &XrState, state: &mut AppState) {
        let Some(my_view) = self.view() else {
            log::error!("Failed to get view for overlay");
            return;
        };

        self.data.inner = {
            let extent = my_view.image().extent();

            let swapchain = xr
                .session
                .create_swapchain(&xr::SwapchainCreateInfo {
                    create_flags: xr::SwapchainCreateFlags::EMPTY,
                    usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                        | xr::SwapchainUsageFlags::SAMPLED,
                    format: state.graphics.native_format as _,
                    sample_count: 1,
                    width: extent[0],
                    height: extent[1],
                    face_count: 1,
                    array_size: 1,
                    mip_count: 1,
                })
                .unwrap();

            let framebuffers = swapchain
                .enumerate_images()
                .unwrap()
                .into_iter()
                .map(|handle| {
                    let vk_image = vk::Image::from_raw(handle);
                    // thanks @yshui
                    let raw_image = unsafe {
                        vulkano::image::sys::RawImage::from_handle(
                            state.graphics.device.clone(),
                            vk_image,
                            ImageCreateInfo {
                                format: state.graphics.native_format,
                                extent,
                                usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_DST,
                                ..Default::default()
                            },
                        )
                        .unwrap()
                    };
                    // SAFETY: OpenXR guarantees that the image is a swapchain image, thus has memory backing it.
                    let image = Arc::new(unsafe { raw_image.assume_bound() });
                    let view = ImageView::new_default(image).unwrap();

                    // HACK: maybe not create one pipeline per image?
                    let pipeline = state.graphics.create_pipeline(
                        view.clone(),
                        vert_common::load(state.graphics.device.clone()).unwrap(),
                        frag_srgb::load(state.graphics.device.clone()).unwrap(),
                        state.graphics.native_format,
                    );
                    let set = pipeline.uniform_sampler(0, my_view.clone(), Filter::Linear);
                    let pass = pipeline.create_pass(
                        [view.image().extent()[0] as _, view.image().extent()[1] as _],
                        state.graphics.quad_verts.clone(),
                        state.graphics.quad_indices.clone(),
                        vec![set],
                    );

                    let inner = Framebuffer::new(
                        pipeline.render_pass.clone(),
                        FramebufferCreateInfo {
                            attachments: vec![view.clone()],
                            extent: [view.image().extent()[0] as _, view.image().extent()[1] as _],
                            layers: 1,
                            ..Default::default()
                        },
                    )
                    .unwrap();

                    XrFramebuffer {
                        inner,
                        view,
                        pipeline,
                        pass,
                    }
                })
                .collect();

            Some(XrOverlayData {
                swapchain,
                framebuffers,
                extent,
            })
        };
    }

    pub(super) fn present_xr(
        &mut self,
        xr: &XrState,
        state: &mut AppState,
    ) -> Option<(xr::SwapchainSubImage<xr::Vulkan>, xr::Extent2Df)> {
        if self.data.inner.is_none() {
            self.initialize(xr, state);
            return None;
        }

        let data = self.data.inner.as_mut().unwrap();

        let idx = data.swapchain.acquire_image().unwrap();

        data.swapchain.wait_image(xr::Duration::INFINITE).unwrap();

        let frame = &data.framebuffers[idx as usize];
        let mut command_buffer = state
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin_render_pass(&frame.pipeline);
        command_buffer.run_ref(&frame.pass);
        command_buffer.end_render_pass().build_and_execute_now();

        data.swapchain.release_image().unwrap();

        let extent = xr::Extent2Df {
            width: self.state.width,
            height: (data.extent[1] as f32 / data.extent[0] as f32) * self.state.width,
        };

        Some((
            xr::SwapchainSubImage::new()
                .swapchain(&data.swapchain)
                .image_rect(xr::Rect2Di {
                    offset: xr::Offset2Di { x: 0, y: 0 },
                    extent: xr::Extent2Di {
                        width: data.extent[0] as _,
                        height: data.extent[1] as _,
                    },
                })
                .image_array_index(0),
            extent,
        ))
    }
}

struct XrOverlayData {
    swapchain: xr::Swapchain<xr::Vulkan>,
    extent: [u32; 3],
    framebuffers: Vec<XrFramebuffer>,
}

struct XrFramebuffer {
    inner: Arc<Framebuffer>,
    view: Arc<ImageView>,
    pipeline: Arc<WlxPipeline>,
    pass: WlxPass,
}
