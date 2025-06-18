use glam::Vec3A;
use wgui::{
    parser::parse_color_hex,
    taffy::{
        self,
        prelude::{length, percent},
    },
    widget::{
        rectangle::{Rectangle, RectangleParams},
        util::WLength,
    },
};

use crate::{
    backend::overlay::{OverlayData, OverlayState, Positioning, Z_ORDER_WATCH, ui_transform},
    gui::panel::GuiPanel,
    state::AppState,
};

pub const WATCH_NAME: &str = "watch";

pub fn create_watch<O>(app: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let mut panel = GuiPanel::new_blank(app, 2048)?;

    let (_, _) = panel.layout.add_child(
        panel.layout.root_widget,
        Rectangle::create(RectangleParams {
            color: wgui::drawing::Color::new(0., 0., 0., 0.5),
            border_color: parse_color_hex("#00ffff").unwrap(),
            border: 2.0,
            round: WLength::Units(4.0),
            ..Default::default()
        })
        .unwrap(),
        taffy::Style {
            size: taffy::Size {
                width: percent(1.0),
                height: percent(1.0),
            },
            align_items: Some(taffy::AlignItems::Center),
            justify_content: Some(taffy::JustifyContent::Center),
            padding: length(4.0),
            ..Default::default()
        },
    )?;

    let positioning = Positioning::FollowHand {
        hand: app.session.config.watch_hand as _,
        lerp: 1.0,
    };

    Ok(OverlayData {
        state: OverlayState {
            name: WATCH_NAME.into(),
            want_visible: true,
            interactable: true,
            z_order: Z_ORDER_WATCH,
            spawn_scale: 0.115, //TODO:configurable
            spawn_point: app.session.config.watch_pos,
            spawn_rotation: app.session.config.watch_rot,
            interaction_transform: ui_transform([400, 200]),
            positioning,
            ..Default::default()
        },
        backend: Box::new(panel),
        ..Default::default()
    })
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
