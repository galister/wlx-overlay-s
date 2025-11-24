use std::{collections::HashMap, rc::Rc, time::Duration};

use glam::{Affine3A, Vec3, Vec3A};
use wgui::{
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables},
    parser::Fetchable,
};
use wlx_common::windowing::{OverlayWindowState, Positioning};

use crate::{
    gui::{
        panel::{GuiPanel, NewGuiPanelParams},
        timer::GuiTimer,
    },
    state::AppState,
    windowing::{
        Z_ORDER_WATCH,
        backend::OverlayEventData,
        window::{OverlayWindowConfig, OverlayWindowData},
    },
};

pub const WATCH_NAME: &str = "watch";

#[derive(Default)]
struct WatchState {
    current_set: Option<usize>,
    set_buttons: Vec<Rc<ComponentButton>>,
}

#[allow(clippy::significant_drop_tightening)]
pub fn create_watch(app: &mut AppState, num_sets: usize) -> anyhow::Result<OverlayWindowConfig> {
    let state = WatchState::default();
    let mut panel = GuiPanel::new_from_template(
        app,
        "gui/watch.xml",
        state,
        NewGuiPanelParams {
            on_custom_id: Some(Box::new(
                move |id, widget, doc_params, layout, parser_state, state| {
                    if &*id != "sets" {
                        return Ok(());
                    }

                    for idx in 0..num_sets {
                        let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                        params.insert("display".into(), (idx + 1).to_string().into());
                        params.insert("handle".into(), idx.to_string().into());
                        parser_state
                            .instantiate_template(doc_params, "Set", layout, widget, params)?;

                        let button_id = format!("set_{idx}");
                        let component =
                            parser_state.fetch_component_as::<ComponentButton>(&button_id)?;
                        state.set_buttons.push(component);
                    }
                    Ok(())
                },
            )),
            ..Default::default()
        },
    )?;

    panel.on_notify = Some(Box::new(|panel, _app, event_data| {
        match event_data {
            OverlayEventData::SetChanged(current_set) => {
                let mut alterables = EventAlterables::default();
                let mut common = CallbackDataCommon {
                    alterables: &mut alterables,
                    state: &panel.layout.state,
                };
                if let Some(old_set) = panel.state.current_set.take() {
                    panel.state.set_buttons[old_set].set_sticky_state(&mut common, false);
                }
                if let Some(new_set) = current_set {
                    panel.state.set_buttons[new_set].set_sticky_state(&mut common, true);
                }
                panel.state.current_set = current_set;
                panel.layout.process_alterables(alterables)?;
            }
        }
        Ok(())
    }));

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: app.session.config.watch_hand,
        lerp: 1.0,
    };

    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: WATCH_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                app.session.config.watch_rot,
                app.session.config.watch_pos,
            ),
            ..OverlayWindowState::default()
        },
        show_on_spawn: true,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

pub fn watch_fade<D>(app: &mut AppState, watch: &mut OverlayWindowData<D>) {
    let Some(state) = watch.config.active_state.as_mut() else {
        return;
    };

    let to_hmd = (state.transform.translation - app.input_state.hmd.translation).normalize();
    let watch_normal = state.transform.transform_vector3a(Vec3A::NEG_Z).normalize();
    let dot = to_hmd.dot(watch_normal);

    state.alpha = (dot - app.session.config.watch_view_angle_min)
        / (app.session.config.watch_view_angle_max - app.session.config.watch_view_angle_min);
    state.alpha += 0.1;
    state.alpha = state.alpha.clamp(0., 1.);
}
