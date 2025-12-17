use std::sync::Arc;

use ash::vk;
use openxr as xr;

use smallvec::SmallVec;
use vulkano::{
    Handle,
    image::{
        ImageCreateInfo, ImageUsage,
        sys::RawImage,
        view::{ImageView, ImageViewCreateInfo},
    },
};
use wgui::gfx::WGfx;

use super::XrState;

#[derive(Default)]
pub(super) struct SwapchainOpts {
    pub immutable: bool,
}

impl SwapchainOpts {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }
}

pub(super) fn create_swapchain(
    xr: &XrState,
    gfx: Arc<WGfx>,
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
        format: gfx.surface_format as _,
        sample_count: 1,
        width: extent[0],
        height: extent[1],
        face_count: 1,
        array_size: extent[2],
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
                    gfx.device.clone(),
                    vk_image,
                    ImageCreateInfo {
                        format: gfx.surface_format as _,
                        extent: [extent[0], extent[1], 1],
                        array_layers: extent[2],
                        usage: ImageUsage::COLOR_ATTACHMENT,
                        ..Default::default()
                    },
                )?
            };
            // SAFETY: OpenXR guarantees that the image is a swapchain image, thus has memory backing it.
            let image = Arc::new(unsafe { raw_image.assume_bound() });
            let mut wsi = WlxSwapchainImage::default();
            for d in 0..extent[2] {
                let mut create_info = ImageViewCreateInfo::from_image(&*image);
                create_info.subresource_range.array_layers = d..d + 1;
                wsi.views.push(ImageView::new(image.clone(), create_info)?);
            }
            Ok(wsi)
        })
        .collect::<anyhow::Result<SmallVec<[WlxSwapchainImage; 4]>>>()?;

    Ok(WlxSwapchain {
        acquired: false,
        ever_acquired: false,
        swapchain,
        images,
        extent,
    })
}

#[derive(Default, Clone)]
pub(super) struct WlxSwapchainImage {
    pub views: SmallVec<[Arc<ImageView>; 2]>,
}

pub(super) struct WlxSwapchain {
    acquired: bool,
    pub(super) ever_acquired: bool,
    pub(super) swapchain: xr::Swapchain<xr::Vulkan>,
    pub(super) extent: [u32; 3],
    pub(super) images: SmallVec<[WlxSwapchainImage; 4]>,
}

impl WlxSwapchain {
    pub(super) fn acquire_wait_image(&mut self) -> anyhow::Result<WlxSwapchainImage> {
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

    pub(super) fn get_subimage(&self, array_index: u32) -> xr::SwapchainSubImage<'_, xr::Vulkan> {
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
            .image_array_index(array_index)
    }
}
