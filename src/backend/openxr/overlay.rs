use openxr as xr;
use std::sync::Arc;
use xr::{CompositionLayerFlags, EyeVisibility};

use super::{swapchain::SwapchainRenderData, transform_to_posef, XrState};
use crate::{
    backend::{openxr::swapchain::create_swapchain_render_data, overlay::OverlayData},
    graphics::WlxCommandBuffer,
};
use vulkano::image::view::ImageView;

#[derive(Default)]
pub struct OpenXrOverlayData {
    last_view: Option<Arc<ImageView>>,
    pub(super) swapchain: Option<SwapchainRenderData>,
    pub(super) init: bool,
}

impl OverlayData<OpenXrOverlayData> {
    pub(super) fn present_xr<'a>(
        &'a mut self,
        xr: &'a XrState,
        command_buffer: &mut WlxCommandBuffer,
    ) -> Option<xr::CompositionLayerQuad<xr::Vulkan>> {
        if let Some(new_view) = self.view() {
            self.data.last_view = Some(new_view);
        }

        let my_view = if let Some(view) = self.data.last_view.as_ref() {
            view.clone()
        } else {
            log::warn!("{}: Will not show - image not ready", self.state.name);
            return None;
        };

        let data = self.data.swapchain.get_or_insert_with(|| {
            let extent = self.backend.extent();
            let srd = create_swapchain_render_data(xr, command_buffer.graphics.clone(), extent);

            log::info!(
                "{}: Created swapchain {}x{}, {} images, {} MB",
                self.state.name,
                extent[0],
                extent[1],
                srd.images.len(),
                extent[0] * extent[1] * 4 * srd.images.len() as u32 / 1024 / 1024
            );
            srd
        });

        let sub_image = data.acquire_present_release(command_buffer, my_view);
        let posef = transform_to_posef(&self.state.transform);

        let scale_x = self.state.transform.matrix3.col(0).length();
        log::info!("{}: scale_x = {}", self.state.name, scale_x);
        let aspect_ratio = self.backend.extent()[1] as f32 / self.backend.extent()[0] as f32;
        let scale_y = scale_x * aspect_ratio;

        let quad = xr::CompositionLayerQuad::new()
            .pose(posef)
            .sub_image(sub_image)
            .eye_visibility(EyeVisibility::BOTH)
            .layer_flags(CompositionLayerFlags::CORRECT_CHROMATIC_ABERRATION)
            .space(&xr.stage)
            .size(xr::Extent2Df {
                width: scale_x,
                height: scale_y,
            });
        Some(quad)
    }
}
