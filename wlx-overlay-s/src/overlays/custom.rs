use std::sync::Arc;

use glam::Vec3A;

use crate::{
    backend::overlay::{OverlayBackend, OverlayState},
    gui::panel::GuiPanel,
    state::AppState,
};

const SETTINGS_NAME: &str = "settings";

pub fn create_custom(
    app: &mut AppState,
    name: Arc<str>,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    return None;

    unreachable!();

    let panel = GuiPanel::new_blank(app, ()).ok()?;
    panel.update_layout().ok()?;

    let state = OverlayState {
        name,
        want_visible: true,
        interactable: true,
        grabbable: true,
        spawn_scale: 0.1, //TODO: this
        spawn_point: Vec3A::from_array([0., 0., -0.5]),
        //interaction_transform: ui_transform(config.size),
        ..Default::default()
    };
    let backend = Box::new(panel);

    Some((state, backend))
}
