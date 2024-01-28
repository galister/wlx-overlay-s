use std::sync::Arc;

use super::XrState;
use crate::{backend::overlay::OverlayData, graphics::WlxPipeline, state::AppState};
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
    last_view: Option<Arc<ImageView>>,
    inner: Option<XrOverlayData>,
    pub(super) init: bool,
}

impl OverlayData<OpenXrOverlayData> {
    pub(super) fn present_xr(
        &mut self,
        xr: &XrState,
        state: &mut AppState,
    ) -> Option<(xr::SwapchainSubImage<xr::Vulkan>, xr::Extent2Df)> {
        if let Some(new_view) = self.view() {
            self.data.last_view = Some(new_view);
        }

        let my_view = if let Some(view) = self.data.last_view.as_ref() {
            view.clone()
        } else {
            log::warn!("{}: Will not show - image not ready", self.state.name);
            return None;
        };

        let data = self.data.inner.get_or_insert_with(|| {
            let extent = self.backend.extent();

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

            let framebuffers: Vec<XrFramebuffer> = swapchain
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

                    let shaders = state.graphics.shared_shaders.read().unwrap();

                    let pipeline = state.graphics.create_pipeline(
                        view.clone(),
                        shaders.get("vert_common").unwrap().clone(),
                        shaders.get("frag_srgb").unwrap().clone(),
                        state.graphics.native_format,
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
                    }
                })
                .collect();

            log::info!(
                "{}: Created swapchain {}x{}, {} images, {} MB",
                self.state.name,
                extent[0],
                extent[1],
                framebuffers.len(),
                extent[0] * extent[1] * 4 * framebuffers.len() as u32 / 1024 / 1024
            );

            XrOverlayData {
                swapchain,
                framebuffers,
                extent,
            }
        });

        let idx = data.swapchain.acquire_image().unwrap();

        data.swapchain.wait_image(xr::Duration::INFINITE).unwrap();

        let frame = &data.framebuffers[idx as usize];
        let mut command_buffer = state
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin_render_pass(&frame.pipeline);

        let set = frame
            .pipeline
            .uniform_sampler(0, my_view.clone(), Filter::Linear);
        let pass = frame.pipeline.create_pass(
            [
                my_view.image().extent()[0] as _,
                my_view.image().extent()[1] as _,
            ],
            state.graphics.quad_verts.clone(),
            state.graphics.quad_indices.clone(),
            vec![set],
        );

        command_buffer.run_ref(&pass);
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
}
