use std::sync::Arc;

use ash::vk;
use openxr as xr;

use smallvec::SmallVec;
use vulkano::{
    image::{sys::RawImage, view::ImageView, ImageCreateInfo, ImageUsage},
    Handle,
};

use crate::graphics::{WlxGraphics, SWAPCHAIN_FORMAT};

use super::XrState;

#[derive(Default)]
pub(super) struct SwapchainOpts {
    pub immutable: bool,
}

impl SwapchainOpts {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }
}

pub(super) fn create_swapchain(
    xr: &XrState,
    graphics: Arc<WlxGraphics>,
    extent: [u32; 3],
    opts: SwapchainOpts,
) -> anyhow::Result<WlxSwapchain> {
    let create_flags = if opts.immutable {
        xr::SwapchainCreateFlags::STATIC_IMAGE
    } else {
        xr::SwapchainCreateFlags::EMPTY
    };

    let swapchain = xr.session.create_swapchain(&xr::SwapchainCreateInfo {
        create_flags,
        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::SAMPLED,
        format: SWAPCHAIN_FORMAT as _,
        sample_count: 1,
        width: extent[0],
        height: extent[1],
        face_count: 1,
        array_size: 1,
        mip_count: 1,
    })?;

    let images = swapchain
        .enumerate_images()?
        .into_iter()
        .map(|handle| {
            let vk_image = vk::Image::from_raw(handle);
            // thanks @yshui
            let raw_image = unsafe {
                RawImage::from_handle_borrowed(
                    graphics.device.clone(),
                    vk_image,
                    ImageCreateInfo {
                        format: SWAPCHAIN_FORMAT as _,
                        extent,
                        usage: ImageUsage::COLOR_ATTACHMENT,
                        ..Default::default()
                    },
                )?
            };
            // SAFETY: OpenXR guarantees that the image is a swapchain image, thus has memory backing it.
            let image = Arc::new(unsafe { raw_image.assume_bound() });
            Ok(ImageView::new_default(image)?)
        })
        .collect::<anyhow::Result<SmallVec<[Arc<ImageView>; 4]>>>()?;

    Ok(WlxSwapchain {
        acquired: false,
        ever_acquired: false,
        swapchain,
        images,
        extent,
    })
}

pub(super) struct WlxSwapchain {
    acquired: bool,
    pub(super) ever_acquired: bool,
    pub(super) swapchain: xr::Swapchain<xr::Vulkan>,
    pub(super) extent: [u32; 3],
    pub(super) images: SmallVec<[Arc<ImageView>; 4]>,
}

impl WlxSwapchain {
    pub(super) fn acquire_wait_image(&mut self) -> anyhow::Result<Arc<ImageView>> {
        let idx = self.swapchain.acquire_image()? as usize;
        self.swapchain.wait_image(xr::Duration::INFINITE)?;
        self.ever_acquired = true;
        self.acquired = true;
        Ok(self.images[idx].clone())
    }

    pub(super) fn ensure_image_released(&mut self) -> anyhow::Result<()> {
        if self.acquired {
            self.swapchain.release_image()?;
            self.acquired = false;
        }
        Ok(())
    }

    pub(super) fn get_subimage(&self) -> xr::SwapchainSubImage<xr::Vulkan> {
        debug_assert!(self.ever_acquired, "swapchain was never acquired!");
        xr::SwapchainSubImage::new()
            .swapchain(&self.swapchain)
            .image_rect(xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di {
                    width: self.extent[0] as _,
                    height: self.extent[1] as _,
                },
            })
            .image_array_index(0)
    }
}
