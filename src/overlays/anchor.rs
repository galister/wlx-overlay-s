use glam::Vec3A;
use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::backend::overlay::{OverlayData, OverlayState};
use crate::config::{load_known_yaml, ConfigType};
use crate::gui::modular::{modular_canvas, ModularUiConfig};
use crate::state::AppState;

pub static ANCHOR_NAME: Lazy<Arc<str>> = Lazy::new(|| Arc::from("anchor"));

pub fn create_anchor<O>(state: &AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let config = load_known_yaml::<ModularUiConfig>(ConfigType::Anchor);

    Ok(OverlayData {
        state: OverlayState {
            name: ANCHOR_NAME.clone(),
            want_visible: false,
            interactable: false,
            grabbable: false,
            z_order: 67,
            spawn_scale: config.width,
            spawn_point: Vec3A::NEG_Z * 0.5,
            ..Default::default()
        },
        backend: Box::new(modular_canvas(&config.size, &config.elements, state)?),
        ..Default::default()
    })
}
