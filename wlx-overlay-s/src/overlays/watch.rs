use std::{rc::Rc, time::Duration};

use chrono::Local;
use chrono_tz::Tz;
use glam::Vec3A;
use regex::Regex;
use wgui::{
    event::{self, EventListenerKind},
    i18n::Translation,
    widget::label::WidgetLabel,
};

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
    let state = WatchState {};
    let mut panel = GuiPanel::new_from_template(app, "gui/watch.xml", state)?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let clock_regex = Regex::new(r"^clock([0-9])_([a-z]+)$").unwrap();

    for (id, widget_id) in &panel.parser_state.ids {
        if let Some(cap) = clock_regex.captures(id) {
            let tz_idx: usize = cap.get(1).unwrap().as_str().parse().unwrap(); // safe due to regex
            let tz_str = (tz_idx > 0)
                .then(|| app.session.config.timezones.get(tz_idx - 1))
                .flatten();
            let role = cap.get(2).unwrap().as_str();

            let mut label = panel
                .layout
                .state
                .widgets
                .get_as::<WidgetLabel>(*widget_id)
                .unwrap();

            let format = match role {
                "tz" => {
                    let mut i18n = panel.layout.state.globals.i18n();
                    if let Some(s) =
                        tz_str.and_then(|tz| tz.split('/').next_back().map(|x| x.replace('_', " ")))
                    {
                        label.set_text(&mut i18n, Translation::from_raw_text(&s));
                    } else {
                        label.set_text(&mut i18n, Translation::from_raw_text("Local"));
                    }

                    continue;
                }
                "date" => "%x",
                "dow" => "%A",
                "time" => {
                    if app.session.config.clock_12h {
                        "%I:%M %p"
                    } else {
                        "%H:%M"
                    }
                }
                _ => {
                    let mut i18n = panel.layout.state.globals.i18n();
                    label.set_text(&mut i18n, Translation::from_raw_text("ERR"));
                    continue;
                }
            };

            let clock = ClockState {
                timezone: tz_str.and_then(|tz| {
                    tz.parse()
                        .inspect_err(|e| log::warn!("Invalid timezone: {e:?}"))
                        .ok()
                }),
                format: format.into(),
            };

            panel.listeners.register(
                &mut panel.listener_handles,
                *widget_id,
                EventListenerKind::InternalStateChange,
                Box::new(move |common, data, _, _| {
                    clock_on_tick(&clock, common, data);
                }),
            );
        }
    }

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

struct ClockState {
    timezone: Option<Tz>,
    format: Rc<str>,
}

fn clock_on_tick(
    clock: &ClockState,
    common: &event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    let date_time = clock.timezone.as_ref().map_or_else(
        || format!("{}", Local::now().format(&clock.format)),
        |tz| format!("{}", Local::now().with_timezone(tz).format(&clock.format)),
    );

    let label = data.obj.get_as_mut::<WidgetLabel>();
    label.set_text(&mut common.i18n(), Translation::from_raw_text(&date_time));
}
