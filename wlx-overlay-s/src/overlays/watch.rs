use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use glam::Vec3A;
use smallvec::SmallVec;

use crate::{
    backend::overlay::{OverlayData, OverlayState, Positioning, Z_ORDER_WATCH},
    gui::{panel::GuiPanel, timer::GuiTimer},
    state::AppState,
};

pub const WATCH_NAME: &str = "watch";

struct WatchState {}

#[allow(clippy::significant_drop_tightening)]
pub fn create_watch<O>(app: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let screens = app
        .screens
        .iter()
        .map(|s| s.name.clone())
        .collect::<SmallVec<[Arc<str>; 8]>>();

    let state = WatchState {};
    let mut panel = GuiPanel::new_from_template(
        app,
        "gui/watch.xml",
        state,
        Some(Box::new(
            move |id, widget, doc_params, layout, parser_state, listeners| {
                if &*id != "sets" {
                    return Ok(());
                }

                for (idx, handle) in screens.iter().enumerate() {
                    let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                    params.insert("display".into(), (idx + 1).to_string().into());
                    params.insert("handle".into(), handle.as_ref().into());
                    parser_state.instantiate_template(
                        doc_params, "Set", layout, listeners, widget, params,
                    )?;
                }
                Ok(())
            },
        )),
    )?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: app.session.config.watch_hand as _,
        lerp: 1.0,
    };

    panel.update_layout()?;

    Ok(OverlayData {
        state: OverlayState {
            name: WATCH_NAME.into(),
            want_visible: true,
            interactable: true,
            z_order: Z_ORDER_WATCH,
            spawn_scale: 0.115, //TODO:configurable
            spawn_point: app.session.config.watch_pos,
            spawn_rotation: app.session.config.watch_rot,
            positioning,
            ..Default::default()
        },
        ..OverlayData::from_backend(Box::new(panel))
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
