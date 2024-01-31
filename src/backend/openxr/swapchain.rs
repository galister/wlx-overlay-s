use std::sync::Arc;

use ash::vk;
use openxr as xr;

use smallvec::SmallVec;
use vulkano::{
    image::{sampler::Filter, sys::RawImage, view::ImageView, ImageCreateInfo, ImageUsage},
    Handle,
};

use crate::graphics::{WlxCommandBuffer, WlxGraphics, WlxPipeline, WlxPipelineDynamic};

use super::XrState;

pub(super) fn create_swapchain_render_data(
    xr: &XrState,
    graphics: Arc<WlxGraphics>,
    extent: [u32; 3],
) -> SwapchainRenderData {
    let swapchain = xr
        .session
        .create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | xr::SwapchainUsageFlags::SAMPLED,
            format: graphics.native_format as _,
            sample_count: 1,
            width: extent[0],
            height: extent[1],
            face_count: 1,
            array_size: 1,
            mip_count: 1,
        })
        .unwrap();

    let shaders = graphics.shared_shaders.read().unwrap();
    let pipeline = graphics.create_pipeline_dynamic(
        shaders.get("vert_common").unwrap().clone(),
        shaders.get("frag_srgb").unwrap().clone(),
        graphics.native_format,
    );

    let images = swapchain
        .enumerate_images()
        .unwrap()
        .into_iter()
        .map(|handle| {
            let vk_image = vk::Image::from_raw(handle);
            // thanks @yshui
            let raw_image = unsafe {
                RawImage::from_handle(
                    graphics.device.clone(),
                    vk_image,
                    ImageCreateInfo {
                        format: graphics.native_format,
                        extent,
                        usage: ImageUsage::COLOR_ATTACHMENT,
                        ..Default::default()
                    },
                )
                .unwrap()
            };
            // SAFETY: OpenXR guarantees that the image is a swapchain image, thus has memory backing it.
            let image = Arc::new(unsafe { raw_image.assume_bound() });
            ImageView::new_default(image).unwrap()
        })
        .collect();

    SwapchainRenderData {
        swapchain,
        pipeline,
        images,
        extent,
    }
}

pub(super) struct SwapchainRenderData {
    pub(super) swapchain: xr::Swapchain<xr::Vulkan>,
    pub(super) pipeline: Arc<WlxPipeline<WlxPipelineDynamic>>,
    pub(super) extent: [u32; 3],
    pub(super) images: SmallVec<[Arc<ImageView>; 4]>,
}

impl SwapchainRenderData {
    pub(super) fn acquire_present_release(
        &mut self,
        command_buffer: &mut WlxCommandBuffer,
        view: Arc<ImageView>,
    ) -> xr::SwapchainSubImage<xr::Vulkan> {
        let idx = self.swapchain.acquire_image().unwrap() as usize;
        self.swapchain.wait_image(xr::Duration::INFINITE).unwrap();

        let render_target = &mut self.images[idx];
        command_buffer.begin_rendering(render_target.clone());

        let target_extent = view.image().extent();
        let set = self
            .pipeline
            .uniform_sampler(0, view.clone(), Filter::Linear);
        let pass = self.pipeline.create_pass(
            [target_extent[0] as _, target_extent[1] as _],
            command_buffer.graphics.quad_verts.clone(),
            command_buffer.graphics.quad_indices.clone(),
            vec![set],
        );
        command_buffer.run_ref(&pass);
        command_buffer.end_rendering();

        self.swapchain.release_image().unwrap();

        xr::SwapchainSubImage::new()
            .swapchain(&self.swapchain)
            .image_rect(xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di {
                    width: target_extent[0] as _,
                    height: target_extent[1] as _,
                },
            })
            .image_array_index(0)
    }
}
