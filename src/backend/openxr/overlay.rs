use glam::Vec3A;
use openxr::{self as xr, CompositionLayerFlags};
use std::{f32::consts::PI, sync::Arc};
use xr::EyeVisibility;

use super::{helpers, swapchain::SwapchainRenderData, CompositionLayer, XrState};
use crate::{
    backend::{
        openxr::swapchain::{create_swapchain_render_data, SwapchainOpts},
        overlay::OverlayData,
    },
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
    ) -> anyhow::Result<CompositionLayer> {
        if let Some(new_view) = self.view() {
            self.data.last_view = Some(new_view);
        }

        let my_view = if let Some(view) = self.data.last_view.as_ref() {
            view.clone()
        } else {
            log::warn!("{}: Will not show - image not ready", self.state.name);
            return Ok(CompositionLayer::None);
        };

        let extent = self.extent().unwrap(); // want panic

        let data = match self.data.swapchain {
            Some(ref mut data) => data,
            None => {
                let srd = create_swapchain_render_data(
                    xr,
                    command_buffer.graphics.clone(),
                    extent,
                    SwapchainOpts::new(),
                )?;
                log::debug!(
                    "{}: Created swapchain {}x{}, {} images, {} MB",
                    self.state.name,
                    extent[0],
                    extent[1],
                    srd.images.len(),
                    extent[0] * extent[1] * 4 * srd.images.len() as u32 / 1024 / 1024
                );
                self.data.swapchain = Some(srd);
                self.data.swapchain.as_mut().unwrap() //safe
            }
        };

        let sub_image = data.acquire_present_release(command_buffer, my_view, self.state.alpha)?;

        let aspect_ratio = extent[1] as f32 / extent[0] as f32;
        let (scale_x, scale_y) = if aspect_ratio < 1.0 {
            let major = self.state.transform.matrix3.col(0).length();
            (major, major * aspect_ratio)
        } else {
            let major = self.state.transform.matrix3.col(1).length();
            (major / aspect_ratio, major)
        };

        if let Some(curvature) = self.state.curvature {
            let radius = scale_x / (2.0 * PI * curvature);
            let quat = helpers::transform_to_norm_quat(&self.state.transform);
            let center_point = self.state.transform.translation + quat.mul_vec3a(Vec3A::Z * radius);

            let posef = helpers::translation_rotation_to_posef(center_point, quat);
            let angle = 2.0 * (scale_x / (2.0 * radius));

            let cylinder = xr::CompositionLayerCylinderKHR::new()
                .layer_flags(CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
                .pose(posef)
                .sub_image(sub_image)
                .eye_visibility(EyeVisibility::BOTH)
                .space(&xr.stage)
                .radius(radius)
                .central_angle(angle)
                .aspect_ratio(aspect_ratio);
            Ok(CompositionLayer::Cylinder(cylinder))
        } else {
            let posef = helpers::transform_to_posef(&self.state.transform);
            let quad = xr::CompositionLayerQuad::new()
                .layer_flags(CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
                .pose(posef)
                .sub_image(sub_image)
                .eye_visibility(EyeVisibility::BOTH)
                .space(&xr.stage)
                .size(xr::Extent2Df {
                    width: scale_x,
                    height: scale_y,
                });
            Ok(CompositionLayer::Quad(quad))
        }
    }

    pub(super) fn after_input(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if self.data.last_visible != self.state.want_visible {
            if self.state.want_visible {
                self.backend.resume(app)?;
            } else {
                self.backend.pause(app)?;
            }
        }
        self.data.last_visible = self.state.want_visible;
        Ok(())
    }
}
