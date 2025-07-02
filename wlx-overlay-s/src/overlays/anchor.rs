use glam::Vec3A;
use std::sync::{Arc, LazyLock};

use crate::backend::overlay::{OverlayData, OverlayState, Positioning, Z_ORDER_ANCHOR};
use crate::gui::panel::GuiPanel;
use crate::state::AppState;

pub static ANCHOR_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("anchor"));

pub fn create_anchor<O>(app: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let (panel, _) = GuiPanel::new_from_template(app, "gui/anchor.xml", ())?;

    Ok(OverlayData {
        state: OverlayState {
            name: ANCHOR_NAME.clone(),
            want_visible: false,
            interactable: false,
            grabbable: false,
            z_order: Z_ORDER_ANCHOR,
            spawn_scale: 0.1,
            spawn_point: Vec3A::NEG_Z * 0.5,
            positioning: Positioning::Static,
            ..Default::default()
        },
        ..OverlayData::from_backend(Box::new(panel))
    })
}
