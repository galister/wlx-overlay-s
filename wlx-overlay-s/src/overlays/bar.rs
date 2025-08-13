use crate::{
    backend::overlay::{OverlayData, OverlayState},
    gui::panel::GuiPanel,
    state::AppState,
};

pub const BAR_NAME: &str = "bar";

struct BarState {}

#[allow(clippy::significant_drop_tightening)]
#[allow(clippy::for_kv_map)] // TODO: remove later
#[allow(clippy::match_same_arms)] // TODO: remove later
pub fn create_bar<O>(app: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let state = BarState {};
    let mut panel = GuiPanel::new_from_template(app, "gui/bar.xml", state)?;

    for (id, _widget_id) in &panel.parser_state.ids {
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

    Ok(OverlayData {
        state: OverlayState {
            name: BAR_NAME.into(),
            want_visible: true,
            interactable: true,
            spawn_scale: 0.15,
            ..Default::default()
        },
        ..OverlayData::from_backend(Box::new(panel))
    })
}
