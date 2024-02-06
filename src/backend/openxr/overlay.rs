use openxr as xr;
use std::sync::Arc;
use xr::{CompositionLayerFlags, EyeVisibility};

use super::{helpers, swapchain::SwapchainRenderData, XrState};
use crate::{
    backend::{openxr::swapchain::create_swapchain_render_data, overlay::OverlayData},
    graphics::WlxCommandBuffer,
    state::AppState,
};
use vulkano::image::view::ImageView;

#[derive(Default)]
pub struct OpenXrOverlayData {
    last_view: Option<Arc<ImageView>>,
    last_visible: bool,
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
        let extent = my_view.image().extent();

        let data = self.data.swapchain.get_or_insert_with(|| {
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
        let posef = helpers::transform_to_posef(&self.state.transform);

        let scale_x = self.state.transform.matrix3.col(0).length();
        let aspect_ratio = extent[1] as f32 / extent[0] as f32;
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

    pub(super) fn after_input(&mut self, app: &mut AppState) {
        if self.data.last_visible != self.state.want_visible {
            if self.state.want_visible {
                self.backend.resume(app);
            } else {
                self.backend.pause(app);
            }
        }
        self.data.last_visible = self.state.want_visible;
    }
}
