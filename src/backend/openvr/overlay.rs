use glam::Vec4;
use ovr_overlay::{
    overlay::{OverlayHandle, OverlayManager},
    sys::VRVulkanTextureData_t,
};
use vulkano::{
    command_buffer::{
        synced::{
            SyncCommandBuffer, SyncCommandBufferBuilder, SyncCommandBufferBuilderExecuteCommands,
        },
        AutoCommandBufferBuilder, CommandBufferExecFuture,
    },
    image::{ImageAccess, ImageLayout},
    sync::{future::NowFuture, ImageMemoryBarrier},
    Handle, VulkanObject,
};

use crate::{backend::overlay::OverlayData, graphics::WlxGraphics, state::AppState};

#[derive(Default)]
pub(super) struct OpenVrOverlayData {
    handle: Option<OverlayHandle>,
    last_image: Option<u64>,
    pub(super) visible: bool,
    pub(super) color: Vec4,
    pub(super) curvature: f32,
    pub(super) sort_order: u32,
}

impl OverlayData<OpenVrOverlayData> {
    pub fn initialize(
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

        self.data.handle = Some(handle);

        self.init(app);

        self.upload_width(overlay);
        self.upload_color(overlay);
        self.upload_curvature(overlay);

        handle
    }

    pub fn after_input(&mut self, overlay: &mut OverlayManager, app: &mut AppState) {
        if self.state.want_visible && !self.data.visible {
            self.show(overlay, app);
        } else if !self.state.want_visible && self.data.visible {
            self.hide(overlay);
        }
    }

    pub fn after_render(&mut self, overlay: &mut OverlayManager, graphics: &WlxGraphics) {
        if self.data.visible {
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

    fn upload_color(&self, overlay: &mut OverlayManager) {
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
                a: 1.0,
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
        if let Err(e) = overlay.set_width(handle, self.state.width) {
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

    fn upload_texture(&mut self, overlay: &mut OverlayManager, graphics: &WlxGraphics) {
        let Some(handle) = self.data.handle else {
            log::debug!("{}: No overlay handle", self.state.name);
            return;
        };

        let Some(view) = self.backend.view() else {
            log::debug!("{}: Not rendered", self.state.name);
            return;
        };

        let image = view.image().inner().image.clone();

        let raw_image = image.handle().as_raw();

        if let Some(last_image) = self.data.last_image {
            if last_image == raw_image {
                return;
            }
        }

        let Some(format) = image.format() else {
            panic!("{}: Image format is None", self.state.name);
        };

        let dimensions = image.dimensions();

        let mut texture = VRVulkanTextureData_t {
            m_nImage: raw_image,
            m_nFormat: format as _,
            m_nWidth: dimensions.width(),
            m_nHeight: dimensions.height(),
            m_nSampleCount: image.samples() as u32,
            m_pDevice: graphics.device.handle().as_raw() as *mut _,
            m_pPhysicalDevice: graphics.device.physical_device().handle().as_raw() as *mut _,
            m_pInstance: graphics.instance.handle().as_raw() as *mut _,
            m_pQueue: graphics.queue.handle().as_raw() as *mut _,
            m_nQueueFamilyIndex: graphics.queue.queue_family_index(),
        };

        graphics
            .transition_layout(
                image.clone(),
                ImageLayout::ColorAttachmentOptimal,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();

        log::info!("nImage: {}, nFormat: {:?}, nWidth: {}, nHeight: {}, nSampleCount: {}, nQueueFamilyIndex: {}", texture.m_nImage, format, texture.m_nWidth, texture.m_nHeight, texture.m_nSampleCount, texture.m_nQueueFamilyIndex);
        if let Err(e) = overlay.set_image_vulkan(handle, &mut texture) {
            panic!("Failed to set overlay texture: {}", e);
        }

        graphics
            .transition_layout(
                image,
                ImageLayout::TransferSrcOptimal,
                ImageLayout::ColorAttachmentOptimal,
            )
            .wait(None)
            .unwrap();
    }
}
