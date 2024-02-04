use std::{sync::Arc, time::Instant};

use chrono::Local;
use glam::{vec2, Affine2};

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
        input::PointerMode,
        overlay::{OverlayData, OverlayState, RelativeTo},
    },
    gui::{color_parse, CanvasBuilder, Control},
    state::AppState,
};

use super::{keyboard::KEYBOARD_NAME, toast::Toast};

pub const WATCH_NAME: &str = "watch";

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
    vol_up.on_press = Some(|_control, _data, _app, _| {
        println!("Volume up!"); //TODO
    });

    let vol_dn = canvas.button(327., 52., 46., 32., "-".into());
    vol_dn.on_press = Some(|_control, _data, _app, _| {
        println!("Volume down!"); //TODO
    });

    canvas.bg_color = color_parse("#303030");
    canvas.fg_color = color_parse("#353535");

    let settings = canvas.button(2., 162., 36., 36., "☰".into());
    settings.on_press = Some(|_control, _data, _app, _| {
        println!("Settings!"); //TODO
    });

    canvas.fg_color = color_parse("#CCBBAA");
    canvas.bg_color = color_parse("#406050");
    // Bottom row
    let num_buttons = screens.len() + 1;
    let button_width = 360. / num_buttons as f32;
    let mut button_x = 40.;

    let keyboard = canvas.button(
        button_x + 2.,
        162.,
        button_width - 4.,
        36.,
        KEYBOARD_NAME.into(),
    );
    keyboard.state = Some(WatchButtonState {
        pressed_at: Instant::now(),
        overlay: OverlaySelector::Name(KEYBOARD_NAME.into()),
        mode: PointerMode::Left,
    });

    keyboard.on_press = Some(overlay_button_dn);
    keyboard.on_release = Some(overlay_button_up);
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
            overlay: OverlaySelector::Id(screen.state.id),
            mode: PointerMode::Left,
        });

        button.on_press = Some(overlay_button_dn);
        button.on_release = Some(overlay_button_up);
        button_x += button_width;
    }
    let interaction_transform =
        Affine2::from_translation(vec2(0.5, 0.5)) * Affine2::from_scale(vec2(1., -2.0));

    let relative_to = RelativeTo::Hand(state.session.watch_hand);

    OverlayData {
        state: OverlayState {
            name: WATCH_NAME.into(),
            size: (400, 200),
            want_visible: true,
            interactable: true,
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
    mode: PointerMode,
    overlay: OverlaySelector,
}

fn overlay_button_dn(
    control: &mut Control<(), WatchButtonState>,
    _: &mut (),
    _: &mut AppState,
    mode: PointerMode,
) {
    if let Some(state) = control.state.as_mut() {
        state.pressed_at = Instant::now();
        state.mode = mode;
    }
}

fn overlay_button_up(control: &mut Control<(), WatchButtonState>, _: &mut (), app: &mut AppState) {
    if let Some(state) = control.state.as_ref() {
        let selector = state.overlay.clone();
        if Instant::now()
            .saturating_duration_since(state.pressed_at)
            .as_millis()
            < 2000
        {
            match state.mode {
                PointerMode::Left => {
                    app.tasks.enqueue(TaskType::Overlay(
                        selector,
                        Box::new(|app, o| {
                            o.want_visible = !o.want_visible;
                            if o.recenter {
                                o.show_hide = o.want_visible;
                                o.reset(app, false);
                            }
                        }),
                    ));
                }
                PointerMode::Right => {
                    app.tasks.enqueue(TaskType::Overlay(
                        selector,
                        Box::new(|app, o| {
                            o.recenter = !o.recenter;
                            o.grabbable = o.recenter;
                            o.show_hide = o.recenter;
                            if !o.recenter {
                                app.tasks.enqueue(TaskType::Toast(Toast::new(
                                    format!("{} is now locked in place!", o.name).into(),
                                    "Right-click again to toggle.".into(),
                                )))
                            }
                        }),
                    ));
                }
                PointerMode::Middle => {
                    app.tasks.enqueue(TaskType::Overlay(
                        selector,
                        Box::new(|app, o| {
                            o.interactable = !o.interactable;
                            if !o.interactable {
                                app.tasks.enqueue(TaskType::Toast(Toast::new(
                                    format!("{} is now non-interactable!", o.name).into(),
                                    "Middle-click again to toggle.".into(),
                                )))
                            }
                        }),
                    ));
                }
                _ => {}
            }
        } else {
            app.tasks.enqueue(TaskType::Overlay(
                selector,
                Box::new(|app, o| {
                    o.reset(app, true);
                }),
            ));
        }
    }
}
