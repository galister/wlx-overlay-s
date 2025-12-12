use glam::Vec3A;
use openxr::{self as xr, CompositionLayerFlags};
use std::{f32::consts::PI, sync::Arc};
use vulkano::image::view::ImageView;
use xr::EyeVisibility;

use super::{CompositionLayer, XrState, helpers, swapchain::WlxSwapchain};
use crate::{
    backend::openxr::swapchain::{SwapchainOpts, create_swapchain},
    state::AppState,
    windowing::window::OverlayWindowData,
};

#[derive(Default)]
pub struct OpenXrOverlayData {
    last_visible: bool,
    pub(super) swapchain: Option<WlxSwapchain>,
    pub(super) init: bool,
    pub(super) cur_visible: bool,
    pub(super) last_alpha: f32,
}

impl OverlayWindowData<OpenXrOverlayData> {
    pub(super) fn ensure_swapchain_acquire<'a>(
        &'a mut self,
        app: &AppState,
        xr: &'a XrState,
        extent: [u32; 3],
    ) -> anyhow::Result<Arc<ImageView>> {
        if let Some(swapchain) = self.data.swapchain.as_mut()
            && swapchain.extent == extent
        {
            return swapchain.acquire_wait_image();
        }

        log::debug!(
            "{}: recreating swapchain at {}x{}",
            self.config.name,
            extent[0],
            extent[1],
        );
        let mut swapchain = create_swapchain(xr, app.gfx.clone(), extent, SwapchainOpts::new())?;
        let tgt = swapchain.acquire_wait_image()?;
        self.data.swapchain = Some(swapchain);
        Ok(tgt)
    }

    pub(super) fn present<'a>(
        &'a mut self,
        xr: &'a XrState,
    ) -> anyhow::Result<CompositionLayer<'a>> {
        let Some(swapchain) = self.data.swapchain.as_mut() else {
            log::warn!("{}: swapchain not ready", self.config.name);
            return Ok(CompositionLayer::None);
        };
        if !swapchain.ever_acquired {
            log::warn!("{}: swapchain not rendered", self.config.name);
            return Ok(CompositionLayer::None);
        }
        swapchain.ensure_image_released()?;

        // overlays without active_state don't get queued for present
        let state = self.config.active_state.as_ref().unwrap();

        let sub_image = swapchain.get_subimage();
        let transform = state.transform * self.config.backend.frame_meta().unwrap().transform; // contract

        let aspect_ratio = swapchain.extent[1] as f32 / swapchain.extent[0] as f32;
        let (scale_x, scale_y) = if aspect_ratio < 1.0 {
            let major = transform.matrix3.col(0).length();
            (major, major * aspect_ratio)
        } else {
            let major = transform.matrix3.col(1).length();
            (major / aspect_ratio, major)
        };

        let flags = if state.additive {
            CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA
        } else {
            CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA
                | CompositionLayerFlags::UNPREMULTIPLIED_ALPHA
        };

        if let Some(curvature) = state.curvature {
            let radius = scale_x / (2.0 * PI * curvature);
            let quat = helpers::transform_to_norm_quat(&transform);
            let center_point = transform.translation + quat.mul_vec3a(Vec3A::Z * radius);

            let posef = helpers::translation_rotation_to_posef(center_point, quat);
            let angle = 2.0 * (scale_x / (2.0 * radius));

            let cylinder = xr::CompositionLayerCylinderKHR::new()
                .layer_flags(flags)
                .pose(posef)
                .sub_image(sub_image)
                .eye_visibility(EyeVisibility::BOTH)
                .space(&xr.stage)
                .radius(radius)
                .central_angle(angle)
                .aspect_ratio(aspect_ratio);
            Ok(CompositionLayer::Cylinder(cylinder))
        } else {
            let posef = helpers::transform_to_posef(&transform);
            let quad = xr::CompositionLayerQuad::new()
                .layer_flags(flags)
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
        let want_visible = self
            .config
            .active_state
            .as_ref()
            .is_some_and(|x| x.alpha > 0.05);

        if self.data.last_visible != want_visible {
            if want_visible {
                self.config.backend.resume(app)?;
            } else {
                self.config.backend.pause(app)?;
            }
        }
        self.data.last_visible = want_visible;
        Ok(())
    }
}
