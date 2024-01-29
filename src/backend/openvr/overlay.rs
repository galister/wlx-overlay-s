use glam::Vec4;
use ovr_overlay::{
    overlay::{OverlayHandle, OverlayManager},
    pose::Matrix3x4,
    sys::{ETrackingUniverseOrigin, VRVulkanTextureData_t},
};
use vulkano::{Handle, VulkanObject};

use crate::{
    backend::overlay::{OverlayData, RelativeTo},
    graphics::WlxGraphics,
    state::AppState,
};

#[derive(Default)]
pub(super) struct OpenVrOverlayData {
    pub(super) handle: Option<OverlayHandle>,
    pub(super) last_image: Option<u64>,
    pub(super) visible: bool,
    pub(super) color: Vec4,
    pub(super) curvature: f32,
    pub(super) sort_order: u32,
    pub(crate) width: f32,
    pub(super) relative_to: RelativeTo,
}

impl OverlayData<OpenVrOverlayData> {
    pub(super) fn initialize(
        &mut self,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> OverlayHandle {
        let key = format!("wlx-{}", self.state.name);
        let handle = match overlay.create_overlay(&key, &key) {
            Ok(handle) => handle,
            Err(e) => {
                panic!("Failed to create overlay: {}", e);
            }
        };
        log::debug!("{}: initialize", self.state.name);

        //watch
        if self.state.id == 0 {
            self.data.sort_order = 68;
        }

        self.data.handle = Some(handle);
        self.data.color = Vec4::ONE;

        self.init(app);

        if self.data.width < f32::EPSILON {
            self.data.width = 1.0;
        }

        self.upload_width(overlay);
        self.upload_color(overlay);
        self.upload_curvature(overlay);
        self.upload_sort_order(overlay);

        handle
    }

    pub(super) fn after_input(&mut self, overlay: &mut OverlayManager, app: &mut AppState) {
        if self.state.want_visible && !self.data.visible {
            self.show(overlay, app);
        } else if !self.state.want_visible && self.data.visible {
            self.hide(overlay);
        }
    }

    pub(super) fn after_render(&mut self, overlay: &mut OverlayManager, graphics: &WlxGraphics) {
        if self.data.visible {
            if self.state.dirty {
                self.upload_transform(overlay);
                self.state.dirty = false;
            }
            self.upload_texture(overlay, graphics);
        }
    }

    fn show(&mut self, overlay: &mut OverlayManager, app: &mut AppState) {
        let handle = match self.data.handle {
            Some(handle) => handle,
            None => self.initialize(overlay, app),
        };
        log::debug!("{}: show", self.state.name);
        if let Err(e) = overlay.set_visibility(handle, true) {
            panic!("Failed to show overlay: {}", e);
        }
        self.data.visible = true;
    }

    fn hide(&mut self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            return;
        };
        log::debug!("{}: hide", self.state.name);
        if let Err(e) = overlay.set_visibility(handle, false) {
            panic!("Failed to hide overlay: {}", e);
        }
        self.data.visible = false;
    }

    pub(super) fn upload_color(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_opacity(handle, self.data.color.w) {
            panic!("Failed to set overlay opacity: {}", e);
        }
        if let Err(e) = overlay.set_tint(
            handle,
            ovr_overlay::ColorTint {
                r: self.data.color.x,
                g: self.data.color.y,
                b: self.data.color.z,
                a: self.data.color.w,
            },
        ) {
            panic!("Failed to set overlay tint: {}", e);
        }
    }

    fn upload_width(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_width(handle, self.data.width) {
            panic!("Failed to set overlay width: {}", e);
        }
    }

    fn upload_curvature(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_curvature(handle, self.data.curvature) {
            panic!("Failed to set overlay curvature: {}", e);
        }
    }

    fn upload_sort_order(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };
        if let Err(e) = overlay.set_sort_order(handle, self.data.sort_order) {
            panic!("Failed to set overlay z order: {}", e);
        }
    }

    pub(super) fn upload_transform(&self, overlay: &mut OverlayManager) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };

        let transform = Matrix3x4([
            [
                self.state.transform.matrix3.x_axis.x,
                self.state.transform.matrix3.y_axis.x,
                self.state.transform.matrix3.z_axis.x,
                self.state.transform.translation.x,
            ],
            [
                self.state.transform.matrix3.x_axis.y,
                self.state.transform.matrix3.y_axis.y,
                self.state.transform.matrix3.z_axis.y,
                self.state.transform.translation.y,
            ],
            [
                self.state.transform.matrix3.x_axis.z,
                self.state.transform.matrix3.y_axis.z,
                self.state.transform.matrix3.z_axis.z,
                self.state.transform.translation.z,
            ],
        ]);

        if let Err(e) = overlay.set_transform_absolute(
            handle,
            ETrackingUniverseOrigin::TrackingUniverseStanding,
            &transform,
        ) {
            panic!("Failed to set overlay transform: {}", e);
        }
    }

    pub(super) fn upload_texture(&mut self, overlay: &mut OverlayManager, graphics: &WlxGraphics) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };

        let Some(view) = self.backend.view() else {
            log::debug!("{}: Not rendered", self.state.name);
            return;
        };

        let image = view.image().clone();

        let raw_image = image.handle().as_raw();

        if let Some(last_image) = self.data.last_image {
            if last_image == raw_image {
                return;
            }
        }

        let dimensions = image.extent();
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
            m_pQueue: graphics.queue.handle().as_raw() as *mut _,
            m_nQueueFamilyIndex: graphics.queue.queue_family_index(),
        };
        log::info!(
            "{}: UploadTex {:?}, {}x{}, {:?}",
            self.state.name,
            format,
            texture.m_nWidth,
            texture.m_nHeight,
            image.usage()
        );
        if let Err(e) = overlay.set_image_vulkan(handle, &mut texture) {
            panic!("Failed to set overlay texture: {}", e);
        }
        log::info!("{}: Uploaded texture", self.state.name);
    }
}
