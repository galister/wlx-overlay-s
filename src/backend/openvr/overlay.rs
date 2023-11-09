use ovr_overlay::sys::VRVulkanTextureData_t;

use crate::overlays::OverlayData;

pub(super) struct OpenVrOverlayManager {
    pub(super) overlays: Vec<OpenVrOverlay>,
}

pub(super) struct OpenVrOverlay {
    pub(super) visible: bool,
    pub(super) color: [f32; 4],
    overlay: OverlayData,
    handle: u32,
    ovr_texture: VRVulkanTextureData_t,
}
