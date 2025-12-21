use glam::{Affine3A, Quat, Vec3};
use std::sync::{Arc, LazyLock};
use wlx_common::windowing::{OverlayWindowState, Positioning};

use crate::gui::panel::GuiPanel;
use crate::state::AppState;
use crate::windowing::window::OverlayWindowConfig;
use crate::windowing::{Z_ORDER_ANCHOR, Z_ORDER_HELP};

pub static ANCHOR_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("anchor"));

pub fn create_anchor(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let mut panel = GuiPanel::new_from_template(app, "gui/anchor.xml", (), Default::default())?;
    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: ANCHOR_NAME.clone(),
        z_order: Z_ORDER_ANCHOR,
        default_state: OverlayWindowState {
            interactable: false,
            grabbable: false,
            positioning: Positioning::Anchored,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.1,
                Quat::IDENTITY,
                Vec3::ZERO, // Vec3::NEG_Z * 0.5,
            ),
            ..OverlayWindowState::default()
        },
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

pub static GRAB_HELP_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("grab-help"));

pub fn create_grab_help(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let mut panel = GuiPanel::new_from_template(app, "gui/grab-help.xml", (), Default::default())?;
    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: GRAB_HELP_NAME.clone(),
        z_order: Z_ORDER_HELP,
        default_state: OverlayWindowState {
            interactable: false,
            grabbable: false,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.15,
                Quat::IDENTITY,
                Vec3::ZERO,
            ),
            ..OverlayWindowState::default()
        },
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
