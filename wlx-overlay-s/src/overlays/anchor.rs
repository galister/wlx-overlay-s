use glam::{Affine3A, Quat, Vec3};
use std::sync::{Arc, LazyLock};

use crate::gui::panel::GuiPanel;
use crate::state::AppState;
use crate::windowing::window::{OverlayWindowConfig, OverlayWindowState, Positioning};
use crate::windowing::Z_ORDER_ANCHOR;

pub static ANCHOR_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("anchor"));

pub fn create_anchor(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let mut panel = GuiPanel::new_from_template(app, "gui/anchor.xml", (), None, false)?;
    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: ANCHOR_NAME.clone(),
        z_order: Z_ORDER_ANCHOR,
        default_state: OverlayWindowState {
            interactable: false,
            grabbable: false,
            positioning: Positioning::Static,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.1,
                Quat::IDENTITY,
                Vec3::NEG_Z * 0.5,
            ),
            ..OverlayWindowState::default()
        },
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
