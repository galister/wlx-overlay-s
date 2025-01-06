use glam::Vec3A;

use crate::{
    backend::overlay::{ui_transform, OverlayData, OverlayState, RelativeTo, Z_ORDER_WATCH},
    config::{load_known_yaml, ConfigType},
    gui::{
        canvas::Canvas,
        modular::{modular_canvas, ModularData, ModularUiConfig},
    },
    state::AppState,
};

pub const WATCH_NAME: &str = "watch";

pub fn create_watch<O>(state: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let config = load_known_yaml::<ModularUiConfig>(ConfigType::Watch);

    let relative_to = RelativeTo::Hand(state.session.config.watch_hand as usize);

    Ok(OverlayData {
        state: OverlayState {
            name: WATCH_NAME.into(),
            want_visible: true,
            interactable: true,
            z_order: Z_ORDER_WATCH,
            spawn_scale: config.width,
            spawn_point: state.session.config.watch_pos,
            spawn_rotation: state.session.config.watch_rot,
            interaction_transform: ui_transform(&config.size),
            relative_to,
            ..Default::default()
        },
        backend: Box::new(create_watch_canvas(Some(config), state)?),
        ..Default::default()
    })
}

pub fn create_watch_canvas(
    config: Option<ModularUiConfig>,
    state: &mut AppState,
) -> anyhow::Result<Canvas<(), ModularData>> {
    let config = config.unwrap_or_else(|| load_known_yaml::<ModularUiConfig>(ConfigType::Watch));

    modular_canvas(&config.size, &config.elements, state)
}

pub fn watch_fade<D>(app: &mut AppState, watch: &mut OverlayData<D>)
where
    D: Default,
{
    if watch.state.saved_transform.is_some() {
        watch.state.want_visible = false;
        return;
    }

    let to_hmd = (watch.state.transform.translation - app.input_state.hmd.translation).normalize();
    let watch_normal = watch
        .state
        .transform
        .transform_vector3a(Vec3A::NEG_Z)
        .normalize();
    let dot = to_hmd.dot(watch_normal);

    if dot < app.session.config.watch_view_angle_min {
        watch.state.want_visible = false;
    } else {
        watch.state.want_visible = true;

        watch.state.alpha = (dot - app.session.config.watch_view_angle_min)
            / (app.session.config.watch_view_angle_max - app.session.config.watch_view_angle_min);
        watch.state.alpha += 0.1;
        watch.state.alpha = watch.state.alpha.clamp(0., 1.);
    }
}
