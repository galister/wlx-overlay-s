use std::{sync::Arc, time::Duration};

use glam::{Affine3A, Quat, Vec3, vec3};
use wlx_common::windowing::OverlayWindowState;

use crate::{
    gui::{
        panel::{GuiPanel, NewGuiPanelParams},
        timer::GuiTimer,
    },
    state::AppState,
    windowing::window::{OverlayCategory, OverlayWindowConfig},
};

struct CustomPanelState {}

pub fn create_custom(app: &mut AppState, name: Arc<str>) -> Option<OverlayWindowConfig> {
    let params = NewGuiPanelParams {
        external_xml: true,
        ..NewGuiPanelParams::default()
    };

    let mut panel =
        GuiPanel::new_from_template(app, &format!("gui/{name}.xml"), CustomPanelState {}, params)
            .inspect_err(|e| log::warn!("Error creating '{name}': {e:?}"))
            .ok()?;

    panel
        .update_layout()
        .inspect_err(|e| log::warn!("Error layouting '{name}': {e:?}"))
        .ok()?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let scale = panel.layout.content_size.x / 40.0 * 0.05;

    Some(OverlayWindowConfig {
        name,
        category: OverlayCategory::Panel,
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * scale,
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.5),
            ),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
