use core::f32;
use std::sync::Arc;

use glam::Vec4;
use ovr_overlay::{
    overlay::{OverlayHandle, OverlayManager},
    pose::Matrix3x4,
    sys::{ETrackingUniverseOrigin, VRVulkanTextureData_t},
};
use vulkano::{image::view::ImageView, Handle, VulkanObject};

use crate::{backend::overlay::OverlayData, graphics::WlxGraphics, state::AppState};

use super::helpers::Affine3AConvert;

#[derive(Default)]
pub(super) struct OpenVrOverlayData {
    pub(super) handle: Option<OverlayHandle>,
    pub(super) visible: bool,
    pub(super) color: Vec4,
    pub(crate) width: f32,
    pub(super) override_width: bool,
    pub(super) image_view: Option<Arc<ImageView>>,
    pub(super) image_dirty: bool,
}

impl OverlayData<OpenVrOverlayData> {
    pub(super) fn initialize(
        &mut self,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> anyhow::Result<OverlayHandle> {
        let key = format!("wlx-{}", self.state.name);
        log::debug!("Create overlay with key: {}", &key);
        let handle = match overlay.create_overlay(&key, &key) {
            Ok(handle) => handle,
            Err(e) => {
                panic!("Failed to create overlay: {e}");
            }
        };
        log::debug!("{}: initialize", self.state.name);

        self.data.handle = Some(handle);
        self.data.color = Vec4::ONE;

        self.init(app)?;

        if self.data.width < f32::EPSILON {
            self.data.width = 1.0;
        }

        self.upload_width(overlay);
        self.upload_color(overlay);
        self.upload_alpha(overlay);
        self.upload_curvature(overlay);
        self.upload_sort_order(overlay);

        Ok(handle)
    }

    pub(super) fn ensure_image_allocated(&mut self, app: &mut AppState) -> anyhow::Result<bool> {
        if self.data.image_view.is_some() {
            return Ok(true);
        }
        let Some(meta) = self.backend.frame_meta() else {
            return Ok(false);
        };
        let image = app
            .graphics
            .render_texture(meta.extent[0], meta.extent[1], meta.format)?;
        self.data.image_view = Some(ImageView::new_default(image)?);
        Ok(true)
    }

    pub(super) fn after_input(
        &mut self,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        if self.state.want_visible && !self.data.visible {
            self.show_internal(overlay, app)?;
        } else if !self.state.want_visible && self.data.visible {
            self.hide_internal(overlay, app)?;
        }
        Ok(())
    }

    pub(super) fn after_render(
        &mut self,
        universe: ETrackingUniverseOrigin,
        overlay: &mut OverlayManager,
        graphics: &WlxGraphics,
    ) {
        if self.data.visible {
            if self.state.dirty {
                self.upload_curvature(overlay);

                self.upload_transform(universe, overlay);
                self.upload_alpha(overlay);
                self.state.dirty = false;
            }
            self.upload_texture(overlay, graphics);
        }
    }

    fn show_internal(
        &mut self,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        let handle = match self.data.handle {
            Some(handle) => handle,
            None => self.initialize(overlay, app)?,
        };
        log::debug!("{}: show", self.state.name);
        if let Err(e) = overlay.set_visibility(handle, true) {
            log::error!("{}: Failed to show overlay: {}", self.state.name, e);
        }
        self.data.visible = true;
        self.backend.resume(app)
    }

    fn hide_internal(
        &mut self,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        let Some(handle) = self.data.handle else {
            return Ok(());
        };
        log::debug!("{}: hide", self.state.name);
        if let Err(e) = overlay.set_visibility(handle, false) {
            log::error!("{}: Failed to hide overlay: {}", self.state.name, e);
        }
        self.data.visible = false;
        self.backend.pause(app)
    }

    pub(super) fn upload_alpha(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_opacity(handle, self.state.alpha) {
            log::error!("{}: Failed to set overlay alpha: {}", self.state.name, e);
        }
    }

    pub(super) fn upload_color(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_tint(
            handle,
            ovr_overlay::ColorTint {
                r: self.data.color.x,
                g: self.data.color.y,
                b: self.data.color.z,
                a: self.data.color.w,
            },
        ) {
            log::error!("{}: Failed to set overlay tint: {}", self.state.name, e);
        }
    }

    fn upload_width(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_width(handle, self.data.width) {
            log::error!("{}: Failed to set overlay width: {}", self.state.name, e);
        }
    }

    fn upload_curvature(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_curvature(handle, self.state.curvature.unwrap_or(0.0)) {
            log::error!(
                "{}: Failed to set overlay curvature: {}",
                self.state.name,
                e
            );
        }
    }

    fn upload_sort_order(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_sort_order(handle, self.state.z_order) {
            log::error!("{}: Failed to set overlay z order: {}", self.state.name, e);
        }
    }

    pub(super) fn upload_transform(
        &mut self,
        universe: ETrackingUniverseOrigin,
        overlay: &mut OverlayManager,
    ) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };

        let effective = self.state.transform
            * self
                .backend
                .frame_meta()
                .map(|f| f.transform)
                .unwrap_or_default();

        let transform = Matrix3x4::from_affine(&effective);

        if let Err(e) = overlay.set_transform_absolute(handle, universe, &transform) {
            log::error!(
                "{}: Failed to set overlay transform: {}",
                self.state.name,
                e
            );
        }
    }

    pub(super) fn upload_texture(&mut self, overlay: &mut OverlayManager, graphics: &WlxGraphics) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };

        let Some(view) = self.data.image_view.as_ref() else {
            log::debug!("{}: Not rendered", self.state.name);
            return;
        };

        if !self.data.image_dirty {
            return;
        }
        self.data.image_dirty = false;

        let image = view.image().clone();
        let dimensions = image.extent();
        if !self.data.override_width {
            let new_width = ((dimensions[0] as f32) / (dimensions[1] as f32)).min(1.0);
            if (new_width - self.data.width).abs() > f32::EPSILON {
                log::info!("{}: New width {}", self.state.name, new_width);
                self.data.width = new_width;
                self.upload_width(overlay);
            }
        }

        let raw_image = image.handle().as_raw();
        let format = image.format();

        let mut texture = VRVulkanTextureData_t {
            m_nImage: raw_image,
            m_nFormat: format as _,
            m_nWidth: dimensions[0],
            m_nHeight: dimensions[1],
            m_nSampleCount: image.samples() as u32,
            m_pDevice: graphics.device.handle().as_raw() as *mut _,
            m_pPhysicalDevice: graphics.device.physical_device().handle().as_raw() as *mut _,
            m_pInstance: graphics.instance.handle().as_raw() as *mut _,
            m_pQueue: graphics.graphics_queue.handle().as_raw() as *mut _,
            m_nQueueFamilyIndex: graphics.graphics_queue.queue_family_index(),
        };
        log::trace!(
            "{}: UploadTex {:?}, {}x{}, {:?}",
            self.state.name,
            format,
            texture.m_nWidth,
            texture.m_nHeight,
            image.usage()
        );
        if let Err(e) = overlay.set_image_vulkan(handle, &mut texture) {
            log::error!("{}: Failed to set overlay texture: {}", self.state.name, e);
        }
    }

    pub(super) fn destroy(&mut self, overlay: &mut OverlayManager) {
        if let Some(handle) = self.data.handle {
            log::debug!("{}: destroy", self.state.name);
            if let Err(e) = overlay.destroy_overlay(handle) {
                log::error!("{}: Failed to destroy overlay: {}", self.state.name, e);
            }
        }
    }
}
