use glam::{Affine3A, Vec3};

use crate::{
    gui::panel::GuiPanel,
    state::AppState,
    windowing::window::{OverlayWindowConfig, OverlayWindowState},
};

pub const BAR_NAME: &str = "bar";

struct BarState {}

#[allow(clippy::significant_drop_tightening)]
#[allow(clippy::for_kv_map)] // TODO: remove later
#[allow(clippy::match_same_arms)] // TODO: remove later
pub fn create_bar(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let state = BarState {};
    let mut panel = GuiPanel::new_from_template(app, "gui/bar.xml", state, None)?;

    for (id, _widget_id) in &panel.parser_state.data.ids {
        match id.as_ref() {
            "lock" => {}
            "anchor" => {}
            "mouse" => {}
            "fade" => {}
            "move" => {}
            "resize" => {}
            "inout" => {}
            "delete" => {}
            _ => {}
        }
    }

    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: BAR_NAME.into(),
        default_state: OverlayWindowState {
            interactable: true,
            transform: Affine3A::from_scale(Vec3::ONE * 0.15),
            ..OverlayWindowState::default()
        },
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
