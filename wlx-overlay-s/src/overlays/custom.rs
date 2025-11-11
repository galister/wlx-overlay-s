use std::sync::Arc;

use glam::{vec3, Affine3A, Quat, Vec3};

use crate::{
    gui::panel::GuiPanel,
    state::AppState,
    windowing::window::{OverlayWindowConfig, OverlayWindowState},
};

const SETTINGS_NAME: &str = "settings";

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[allow(dead_code)]
pub fn create_custom(app: &mut AppState, name: Arc<str>) -> Option<OverlayWindowConfig> {
    return None;

    unreachable!();

    let panel = GuiPanel::new_blank(app, (), false).ok()?;
    panel.update_layout().ok()?;

    Some(OverlayWindowConfig {
        name,
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.1, // TODO scale
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.5),
            ),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
