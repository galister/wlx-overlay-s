use std::{sync::Arc, time::Instant};

use chrono::Local;
use glam::{vec2, Affine2};

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
        overlay::{OverlayData, OverlayState, RelativeTo},
    },
    gui::{color_parse, CanvasBuilder},
    state::AppState,
};

pub fn create_watch<O>(state: &AppState, screens: &[OverlayData<O>]) -> OverlayData<O>
where
    O: Default,
{
    let mut canvas = CanvasBuilder::new(400, 200, state.graphics.clone(), state.format, ());
    let empty_str: Arc<str> = Arc::from("");

    // Background
    canvas.bg_color = color_parse("#353535");
    canvas.panel(0., 0., 400., 200.);

    // Time display
    canvas.font_size = 46;
    let clock = canvas.label(19., 100., 200., 50., empty_str.clone());
    clock.on_update = Some(|control, _data, _app| {
        let date = Local::now();
        control.set_text(&format!("{}", &date.format("%H:%M")));
    });

    canvas.font_size = 14;
    let date = canvas.label(20., 125., 200., 50., empty_str.clone());
    date.on_update = Some(|control, _data, _app| {
        let date = Local::now();
        control.set_text(&format!("{}", &date.format("%x")));
    });

    let day_of_week = canvas.label(20., 150., 200., 50., empty_str);
    day_of_week.on_update = Some(|control, _data, _app| {
        let date = Local::now();
        control.set_text(&format!("{}", &date.format("%A")));
    });

    // Volume controls
    canvas.bg_color = color_parse("#222222");
    canvas.fg_color = color_parse("#AAAAAA");
    canvas.font_size = 14;

    canvas.bg_color = color_parse("#303030");
    canvas.fg_color = color_parse("#353535");

    let vol_up = canvas.button(327., 116., 46., 32., "+".into());
    vol_up.on_press = Some(|_control, _data, _app| {
        println!("Volume up!"); //TODO
    });

    let vol_dn = canvas.button(327., 52., 46., 32., "-".into());
    vol_dn.on_press = Some(|_control, _data, _app| {
        println!("Volume down!"); //TODO
    });

    canvas.bg_color = color_parse("#303030");
    canvas.fg_color = color_parse("#353535");

    let settings = canvas.button(2., 162., 36., 36., "☰".into());
    settings.on_press = Some(|_control, _data, _app| {
        println!("Settings!"); //TODO
    });

    canvas.fg_color = color_parse("#CCBBAA");
    canvas.bg_color = color_parse("#406050");
    // Bottom row
    let num_buttons = screens.len() + 1;
    let button_width = 360. / num_buttons as f32;
    let mut button_x = 40.;

    let keyboard = canvas.button(button_x + 2., 162., button_width - 4., 36., "kbd".into());
    keyboard.state = Some(WatchButtonState {
        pressed_at: Instant::now(),
        scr_idx: 0,
    });

    keyboard.on_press = Some(|control, _data, _app| {
        if let Some(state) = control.state.as_mut() {
            state.pressed_at = Instant::now();
        }
    });
    keyboard.on_release = Some(|control, _data, app| {
        if let Some(state) = control.state.as_ref() {
            if Instant::now()
                .saturating_duration_since(state.pressed_at)
                .as_millis()
                < 2000
            {
                app.tasks.enqueue(TaskType::Overlay(
                    OverlaySelector::Name("kbd".into()),
                    Box::new(|_app, o| {
                        o.want_visible = !o.want_visible;
                    }),
                ));
            } else {
                app.tasks.enqueue(TaskType::Overlay(
                    OverlaySelector::Name("kbd".into()),
                    Box::new(|app, o| {
                        o.reset(app);
                    }),
                ));
            }
        }
    });
    button_x += button_width;

    canvas.bg_color = color_parse("#405060");

    for screen in screens.iter() {
        let button = canvas.button(
            button_x + 2.,
            162.,
            button_width - 4.,
            36.,
            screen.state.name.clone(),
        );
        button.state = Some(WatchButtonState {
            pressed_at: Instant::now(),
            scr_idx: screen.state.id,
        });

        button.on_press = Some(|control, _data, _app| {
            if let Some(state) = control.state.as_mut() {
                state.pressed_at = Instant::now();
            }
        });
        button.on_release = Some(|control, _data, app| {
            if let Some(state) = control.state.as_ref() {
                let scr_idx = state.scr_idx;
                if Instant::now()
                    .saturating_duration_since(state.pressed_at)
                    .as_millis()
                    < 2000
                {
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Id(scr_idx),
                        Box::new(|_app, o| {
                            o.want_visible = !o.want_visible;
                        }),
                    ));
                } else {
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Id(scr_idx),
                        Box::new(|app, o| {
                            o.reset(app);
                        }),
                    ));
                }
            }
        });
        button_x += button_width;
    }
    let interaction_transform =
        Affine2::from_translation(vec2(0.5, 0.5)) * Affine2::from_scale(vec2(1., -2.0));

    let relative_to = RelativeTo::Hand(state.session.watch_hand);

    OverlayData {
        state: OverlayState {
            name: "Watch".into(),
            size: (400, 200),
            want_visible: true,
            spawn_scale: 0.11 * state.session.config.watch_scale,
            spawn_point: state.session.watch_pos.into(),
            spawn_rotation: state.session.watch_rot,
            interaction_transform,
            relative_to,
            ..Default::default()
        },
        backend: Box::new(canvas.build()),
        ..Default::default()
    }
}

struct WatchButtonState {
    pressed_at: Instant,
    scr_idx: usize,
}
