use glam::{Quat, Vec3A};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    backend::overlay::{ui_transform, OverlayData, OverlayState, RelativeTo},
    config::{
        def_half, def_left, def_point7, def_watch_pos, def_watch_rot, load_known_yaml, ConfigType,
    },
    config_io,
    gui::modular::{modular_canvas, ModularUiConfig},
    state::{AppState, LeftRight},
};

pub const WATCH_NAME: &str = "watch";

pub fn create_watch<O>(state: &AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let config = load_known_yaml::<ModularUiConfig>(ConfigType::Watch);

    let canvas = modular_canvas(&config.size, &config.elements, state)?;

    let relative_to = RelativeTo::Hand(state.session.config.watch_hand as usize);

    Ok(OverlayData {
        state: OverlayState {
            name: WATCH_NAME.into(),
            want_visible: true,
            interactable: true,
            spawn_scale: config.width,
            spawn_point: Vec3A::from_slice(&state.session.config.watch_pos),
            spawn_rotation: Quat::from_slice(&state.session.config.watch_rot),
            interaction_transform: ui_transform(&config.size),
            relative_to,
            ..Default::default()
        },
        backend: Box::new(canvas),
        ..Default::default()
    })
}

pub fn watch_fade<D>(app: &mut AppState, watch: &mut OverlayData<D>)
where
    D: Default,
{
    if watch.state.saved_scale.is_some_and(|s| s < f32::EPSILON) {
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

#[derive(Deserialize, Serialize)]
pub struct WatchConf {
    #[serde(default = "def_watch_pos")]
    pub watch_pos: [f32; 3],

    #[serde(default = "def_watch_rot")]
    pub watch_rot: [f32; 4],

    #[serde(default = "def_left")]
    pub watch_hand: LeftRight,

    #[serde(default = "def_half")]
    pub watch_view_angle_min: f32,

    #[serde(default = "def_point7")]
    pub watch_view_angle_max: f32,
}

fn get_config_path() -> PathBuf {
    let mut path = config_io::get_conf_d_path();
    path.push("watch_state.yaml");
    path
}
pub fn save_watch(app: &mut AppState) -> anyhow::Result<()> {
    let conf = WatchConf {
        watch_pos: app.session.config.watch_pos,
        watch_rot: app.session.config.watch_rot,
        watch_hand: app.session.config.watch_hand,
        watch_view_angle_min: app.session.config.watch_view_angle_min,
        watch_view_angle_max: app.session.config.watch_view_angle_max,
    };

    let yaml = serde_yaml::to_string(&conf)?;
    std::fs::write(get_config_path(), yaml)?;

    Ok(())
}
