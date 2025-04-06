use std::sync::Arc;

use glam::Vec3A;

use crate::{
    backend::overlay::{ui_transform, OverlayBackend, OverlayState},
    config::{load_custom_ui, load_known_yaml, ConfigType},
    gui::modular::{modular_canvas, ModularUiConfig},
    state::AppState,
};

const SETTINGS_NAME: &str = "settings";

pub fn create_custom(
    state: &mut AppState,
    name: Arc<str>,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    let config = if &*name == SETTINGS_NAME {
        load_known_yaml::<ModularUiConfig>(ConfigType::Settings)
    } else {
        match load_custom_ui(&name) {
            Ok(config) => config,
            Err(e) => {
                log::error!("Failed to load custom UI config for {name}: {e:?}");
                return None;
            }
        }
    };

    let canvas = match modular_canvas(config.size, &config.elements, state) {
        Ok(canvas) => canvas,
        Err(e) => {
            log::error!("Failed to create canvas for {name}: {e:?}");
            return None;
        }
    };

    let state = OverlayState {
        name,
        want_visible: true,
        interactable: true,
        grabbable: true,
        spawn_scale: config.width,
        spawn_point: Vec3A::from_array(config.spawn_pos.unwrap_or([0., 0., -0.5])),
        interaction_transform: ui_transform(config.size),
        ..Default::default()
    };
    let backend = Box::new(canvas);

    Some((state, backend))
}
