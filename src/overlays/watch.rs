use std::{
    f32::consts::PI,
    io::Read,
    process::{self, Stdio},
    sync::Arc,
    time::Instant,
};

use chrono::Local;
use chrono_tz::Tz;
use glam::{vec2, Affine2, Quat, Vec3, Vec3A};
use serde::Deserialize;

use crate::{
    backend::{
        common::{OverlayContainer, OverlaySelector, TaskType},
        input::PointerMode,
        overlay::{OverlayData, OverlayState, RelativeTo},
    },
    config::load_watch,
    gui::{color_parse, CanvasBuilder, Control},
    state::AppState,
};

use super::{keyboard::KEYBOARD_NAME, toast::Toast};

const FALLBACK_COLOR: Vec3 = Vec3 {
    x: 1.,
    y: 0.,
    z: 1.,
};

pub const WATCH_NAME: &str = "watch";
pub const WATCH_SCALE: f32 = 0.11;

pub fn create_watch<O>(state: &AppState, screens: &[OverlayData<O>]) -> OverlayData<O>
where
    O: Default,
{
    let config = load_watch();

    let mut canvas = CanvasBuilder::new(
        config.watch_size[0] as _,
        config.watch_size[1] as _,
        state.graphics.clone(),
        state.format,
        (),
    );
    let empty_str: Arc<str> = Arc::from("");

    for elem in config.watch_elements.into_iter() {
        match elem {
            WatchElement::Panel {
                rect: [x, y, w, h],
                bg_color,
            } => {
                canvas.bg_color = color_parse(&bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.panel(x, y, w, h);
            }
            WatchElement::Label {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                text,
            } => {
                canvas.font_size = font_size;
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.label(x, y, w, h, text);
            }
            WatchElement::Clock {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                format,
                timezone,
            } => {
                canvas.font_size = font_size;
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);

                let tz: Option<Tz> = match timezone {
                    Some(tz) => Some(tz.parse().unwrap_or_else(|_| {
                        log::error!("Failed to parse timezone '{}'", &tz);
                        canvas.fg_color = FALLBACK_COLOR;
                        Tz::UTC
                    })),
                    None => None,
                };

                let label = canvas.label(x, y, w, h, empty_str.clone());
                label.state = Some(ElemState::Clock {
                    timezone: tz,
                    format,
                });
                label.on_update = Some(clock_update);
            }
            WatchElement::ExecLabel {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                exec,
                interval,
            } => {
                canvas.font_size = font_size;
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                let label = canvas.label(x, y, w, h, empty_str.clone());
                label.state = Some(ElemState::AutoExec {
                    last_exec: Instant::now(),
                    interval,
                    exec,
                    child: None,
                });
                label.on_update = Some(exec_label_update);
            }
            WatchElement::ExecButton {
                rect: [x, y, w, h],
                font_size,
                bg_color,
                fg_color,
                exec,
                text,
            } => {
                canvas.bg_color = color_parse(&bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = font_size;
                let button = canvas.button(x, y, w, h, text.clone());
                button.state = Some(ElemState::ExecButton { exec, child: None });
                button.on_press = Some(exec_button);
            }
            WatchElement::FuncButton {
                rect: [x, y, w, h],
                font_size,
                bg_color,
                fg_color,
                func,
                func_right,
                func_middle,
                text,
            } => {
                canvas.bg_color = color_parse(&bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = font_size;
                let button = canvas.button(x, y, w, h, text.clone());
                button.state = Some(ElemState::FuncButton {
                    func,
                    func_right,
                    func_middle,
                });
                button.on_press = Some(btn_func_dn);
            }
            WatchElement::Batteries {
                rect: [x, y, w, h],
                font_size,
                num_devices,
                normal_fg_color,
                layout,
                ..
            } => {
                let num_buttons = num_devices as f32;
                let mut button_x = x;
                let mut button_y = y;
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (w / num_buttons, h),
                    ListLayout::Vertical => (w, h / num_buttons),
                };

                canvas.font_size = font_size;
                canvas.fg_color = color_parse(&normal_fg_color).unwrap_or(FALLBACK_COLOR);

                for i in 0..num_devices {
                    let label = canvas.label_centered(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        empty_str.clone(),
                    );
                    label.state = Some(ElemState::Battery { device: i as _ });
                    label.on_update = Some(battery_update);

                    button_x += match layout {
                        ListLayout::Horizontal => button_w,
                        ListLayout::Vertical => 0.,
                    };
                    button_y += match layout {
                        ListLayout::Horizontal => 0.,
                        ListLayout::Vertical => button_h,
                    };
                }
            }
            WatchElement::KeyboardButton {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                bg_color,
                text,
            } => {
                canvas.bg_color = color_parse(&bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = font_size;

                let keyboard = canvas.button(x, y, w, h, text);
                keyboard.state = Some(ElemState::OverlayButton {
                    pressed_at: Instant::now(),
                    mode: PointerMode::Left,
                    overlay: OverlaySelector::Name(KEYBOARD_NAME.into()),
                });
                keyboard.on_press = Some(overlay_button_dn);
                keyboard.on_release = Some(overlay_button_up);
                keyboard.on_scroll = Some(overlay_button_scroll);
            }
            WatchElement::OverlayList {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                bg_color,
                layout,
            } => {
                let num_buttons = screens.len() as f32;
                let mut button_x = x;
                let mut button_y = y;
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (w / num_buttons, h),
                    ListLayout::Vertical => (w, h / num_buttons),
                };

                canvas.bg_color = color_parse(&bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = font_size;

                for screen in screens.iter() {
                    let button = canvas.button(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        screen.state.name.clone(),
                    );
                    button.state = Some(ElemState::OverlayButton {
                        pressed_at: Instant::now(),
                        mode: PointerMode::Left,
                        overlay: OverlaySelector::Id(screen.state.id),
                    });

                    button.on_press = Some(overlay_button_dn);
                    button.on_release = Some(overlay_button_up);
                    button.on_scroll = Some(overlay_button_scroll);

                    button_x += match layout {
                        ListLayout::Horizontal => button_w,
                        ListLayout::Vertical => 0.,
                    };
                    button_y += match layout {
                        ListLayout::Horizontal => 0.,
                        ListLayout::Vertical => button_h,
                    };
                }
            }
        }
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
            spawn_scale: WATCH_SCALE * state.session.config.watch_scale,
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

enum ElemState {
    Battery {
        device: usize,
    },
    Clock {
        timezone: Option<Tz>,
        format: Arc<str>,
    },
    AutoExec {
        last_exec: Instant,
        interval: f32,
        exec: Vec<Arc<str>>,
        child: Option<process::Child>,
    },
    OverlayButton {
        pressed_at: Instant,
        mode: PointerMode,
        overlay: OverlaySelector,
    },
    ExecButton {
        exec: Vec<Arc<str>>,
        child: Option<process::Child>,
    },
    FuncButton {
        func: ButtonFunc,
        func_right: Option<ButtonFunc>,
        func_middle: Option<ButtonFunc>,
    },
}

fn btn_func_dn(
    control: &mut Control<(), ElemState>,
    _: &mut (),
    app: &mut AppState,
    mode: PointerMode,
) {
    let ElemState::FuncButton {
        func,
        func_right,
        func_middle,
    } = control.state.as_ref().unwrap()
    else {
        log::error!("FuncButton state not found");
        return;
    };

    let func = match mode {
        PointerMode::Left => func,
        PointerMode::Right => func_right.as_ref().unwrap_or(func),
        PointerMode::Middle => func_middle.as_ref().unwrap_or(func),
        _ => return,
    };

    match func {
        ButtonFunc::HideWatch => {
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(|app, o| {
                    o.want_visible = false;
                    o.spawn_scale = 0.0;
                    app.tasks.enqueue(TaskType::Toast(Toast::new(
                        "Watch hidden".into(),
                        "Use show/hide button to restore.".into(),
                    )))
                }),
            ));
        }
        ButtonFunc::SwitchWatchHand => {
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(|app, o| {
                    if let RelativeTo::Hand(0) = o.relative_to {
                        o.relative_to = RelativeTo::Hand(1);
                        o.spawn_rotation = app.session.watch_rot;
                        o.spawn_rotation = app.session.watch_rot
                            * Quat::from_rotation_x(PI)
                            * Quat::from_rotation_z(PI);
                        o.spawn_point = app.session.watch_pos.into();
                        o.spawn_point.x *= -1.;
                    } else {
                        o.relative_to = RelativeTo::Hand(0);
                        o.spawn_rotation = app.session.watch_rot;
                        o.spawn_point = app.session.watch_pos.into();
                    }
                    app.tasks.enqueue(TaskType::Toast(Toast::new(
                        "Watch switched".into(),
                        "Check your other hand".into(),
                    )))
                }),
            ));
        }
    }
}

fn battery_update(control: &mut Control<(), ElemState>, _: &mut (), app: &mut AppState) {
    let ElemState::Battery { device } = control.state.as_ref().unwrap() else {
        return;
    };
    let device = app.input_state.devices.get(*device);

    let tags = ["", "H", "L", "R", "T"];

    let text = match device {
        Some(d) => d
            .soc
            .map(|soc| format!("{}{}", tags[d.role as usize], (soc * 100.).min(99.) as u32))
            .unwrap_or_else(|| "".into()),
        None => "".into(),
    };

    control.set_text(&text);
}

fn exec_button(
    control: &mut Control<(), ElemState>,
    _: &mut (),
    _: &mut AppState,
    _mode: PointerMode,
) {
    let ElemState::ExecButton {
        exec,
        ref mut child,
        ..
    } = control.state.as_mut().unwrap()
    else {
        log::error!("ExecButton state not found");
        return;
    };
    if let Some(proc) = child {
        match proc.try_wait() {
            Ok(Some(code)) => {
                if !code.success() {
                    log::error!("Child process exited with code: {}", code);
                }
                *child = None;
            }
            Ok(None) => {
                log::warn!("Unable to launch child process: previous child not exited yet");
                return;
            }
            Err(e) => {
                *child = None;
                log::error!("Error checking child process: {:?}", e);
            }
        }
    }
    let args = exec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();
    match process::Command::new(args[0]).args(&args[1..]).spawn() {
        Ok(proc) => {
            *child = Some(proc);
        }
        Err(e) => {
            log::error!("Failed to spawn process {:?}: {:?}", args, e);
        }
    };
}

fn exec_label_update(control: &mut Control<(), ElemState>, _: &mut (), _: &mut AppState) {
    let ElemState::AutoExec {
        ref mut last_exec,
        interval,
        exec,
        ref mut child,
    } = control.state.as_mut().unwrap()
    else {
        log::error!("AutoExec state not found");
        return;
    };

    if let Some(mut proc) = child.take() {
        match proc.try_wait() {
            Ok(Some(code)) => {
                if !code.success() {
                    log::error!("Child process exited with code: {}", code);
                } else {
                    if let Some(mut stdout) = proc.stdout.take() {
                        let mut buf = String::new();
                        if let Ok(_) = stdout.read_to_string(&mut buf) {
                            control.set_text(&buf);
                        } else {
                            log::error!("Failed to read stdout for child process");
                            return;
                        }
                        return;
                    } else {
                        log::error!("No stdout for child process");
                        return;
                    }
                }
            }
            Ok(None) => {
                *child = Some(proc);
                // not exited yet
                return;
            }
            Err(e) => {
                *child = None;
                log::error!("Error checking child process: {:?}", e);
                return;
            }
        }
    }

    if Instant::now()
        .saturating_duration_since(*last_exec)
        .as_secs_f32()
        > *interval
    {
        *last_exec = Instant::now();
        let args = exec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();

        match process::Command::new(args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(proc) => {
                *child = Some(proc);
            }
            Err(e) => {
                log::error!("Failed to spawn process {:?}: {:?}", args, e);
            }
        };
    }
}

fn clock_update(control: &mut Control<(), ElemState>, _: &mut (), _: &mut AppState) {
    let ElemState::Clock { timezone, format } = control.state.as_ref().unwrap() else {
        log::error!("Clock state not found");
        return;
    };

    if let Some(tz) = timezone {
        let date = Local::now().with_timezone(tz);
        control.set_text(&format!("{}", &date.format(format)));
    } else {
        let date = Local::now();
        control.set_text(&format!("{}", &date.format(format)));
    }
}

fn overlay_button_scroll(
    control: &mut Control<(), ElemState>,
    _: &mut (),
    app: &mut AppState,
    delta: f32,
) {
    let ElemState::OverlayButton { overlay, .. } = control.state.as_mut().unwrap() else {
        log::error!("OverlayButton state not found");
        return;
    };

    if delta > 0. {
        app.tasks.enqueue(TaskType::Overlay(
            overlay.clone(),
            Box::new(|_, o| {
                o.alpha = (o.alpha + 0.025).min(1.);
                o.dirty = true;
                log::debug!("{}: alpha {}", o.name, o.alpha);
            }),
        ));
    } else {
        app.tasks.enqueue(TaskType::Overlay(
            overlay.clone(),
            Box::new(|_, o| {
                o.alpha = (o.alpha - 0.025).max(0.1);
                o.dirty = true;
                log::debug!("{}: alpha {}", o.name, o.alpha);
            }),
        ));
    }
}

fn overlay_button_dn(
    control: &mut Control<(), ElemState>,
    _: &mut (),
    _: &mut AppState,
    ptr_mode: PointerMode,
) {
    let ElemState::OverlayButton {
        ref mut pressed_at,
        ref mut mode,
        ..
    } = control.state.as_mut().unwrap()
    else {
        log::error!("OverlayButton state not found");
        return;
    };
    *pressed_at = Instant::now();
    *mode = ptr_mode;
}

fn overlay_button_up(control: &mut Control<(), ElemState>, _: &mut (), app: &mut AppState) {
    let ElemState::OverlayButton {
        pressed_at,
        mode,
        overlay,
    } = control.state.as_ref().unwrap()
    else {
        log::error!("OverlayButton state not found");
        return;
    };

    if Instant::now()
        .saturating_duration_since(*pressed_at)
        .as_millis()
        < 2000
    {
        match mode {
            PointerMode::Left => {
                app.tasks.enqueue(TaskType::Overlay(
                    overlay.clone(),
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
                    overlay.clone(),
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
                    overlay.clone(),
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
            overlay.clone(),
            Box::new(|app, o| {
                o.reset(app, true);
            }),
        ));
    }
}

#[derive(Deserialize)]
pub struct WatchConfig {
    watch_hand: LeftRight,
    watch_size: [u32; 2],
    watch_elements: Vec<WatchElement>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "type")]
enum WatchElement {
    Panel {
        rect: [f32; 4],
        bg_color: Arc<str>,
    },
    Label {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        text: Arc<str>,
    },
    Clock {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        format: Arc<str>,
        timezone: Option<Arc<str>>,
    },
    ExecLabel {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        exec: Vec<Arc<str>>,
        interval: f32,
    },
    ExecButton {
        rect: [f32; 4],
        font_size: isize,
        bg_color: Arc<str>,
        fg_color: Arc<str>,
        exec: Vec<Arc<str>>,
        text: Arc<str>,
    },
    Batteries {
        rect: [f32; 4],
        font_size: isize,
        low_threshold: f32,
        num_devices: u16,
        normal_fg_color: Arc<str>,
        normal_bg_color: Arc<str>,
        low_fg_color: Arc<str>,
        low_bg_color: Arc<str>,
        charging_fg_color: Arc<str>,
        charging_bg_color: Arc<str>,
        layout: ListLayout,
    },
    KeyboardButton {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        text: Arc<str>,
    },
    OverlayList {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        layout: ListLayout,
    },
    FuncButton {
        rect: [f32; 4],
        font_size: isize,
        bg_color: Arc<str>,
        fg_color: Arc<str>,
        func: ButtonFunc,
        func_right: Option<ButtonFunc>,
        func_middle: Option<ButtonFunc>,
        text: Arc<str>,
    },
}

#[derive(Deserialize)]
enum ButtonFunc {
    HideWatch,
    SwitchWatchHand,
}

#[derive(Deserialize)]
enum ListLayout {
    Horizontal,
    Vertical,
}

#[derive(Deserialize)]
enum LeftRight {
    Left,
    Right,
}

pub fn watch_fade<D>(app: &mut AppState, overlays: &mut OverlayContainer<D>)
where
    D: Default,
{
    let watch = overlays
        .mut_by_selector(&OverlaySelector::Name(WATCH_NAME.into()))
        .unwrap();

    if watch.state.spawn_scale < f32::EPSILON {
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
