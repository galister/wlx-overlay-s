use std::{
    io::Read,
    process::{self, Stdio},
    sync::Arc,
    time::Instant,
};

use chrono::Local;
use chrono_tz::Tz;
use glam::{vec2, Affine2, Vec3};
use serde::Deserialize;

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
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
                label.state = Some(ElemState {
                    clock: Some(ClockState {
                        timezone: tz,
                        format,
                    }),
                    ..Default::default()
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
                label.state = Some(ElemState {
                    exec: Some(ExecState {
                        last_exec: Instant::now(),
                        interval,
                        exec,
                        child: None,
                    }),
                    button: None,
                    ..Default::default()
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
                button.state = Some(ElemState {
                    exec: Some(ExecState {
                        last_exec: Instant::now(),
                        interval: 0.,
                        exec,
                        child: None,
                    }),
                    button: Some(WatchButtonState {
                        pressed_at: Instant::now(),
                        mode: PointerMode::Left,
                        overlay: None,
                    }),
                    ..Default::default()
                });
                button.on_press = Some(exec_button);
            }
            WatchElement::Batteries {
                rect,
                font_size,
                num_devices,
                normal_fg_color,
                layout,
                ..
            } => {
                let num_buttons = num_devices as f32;
                let mut button_x = rect[0];
                let mut button_y = rect[1];
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (rect[2] / num_buttons, rect[3]),
                    ListLayout::Vertical => (rect[2], rect[3] / num_buttons),
                };

                canvas.font_size = font_size;
                canvas.fg_color = color_parse(&normal_fg_color).unwrap_or(FALLBACK_COLOR);

                for i in 0..num_devices {
                    let label = canvas.label(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        empty_str.clone(),
                    );
                    label.state = Some(ElemState {
                        battery: Some(i as usize),
                        ..Default::default()
                    });
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
            WatchElement::OverlayList {
                rect,
                font_size,
                kbd_fg_color,
                kbd_bg_color,
                scr_fg_color,
                scr_bg_color,
                layout,
            } => {
                let num_buttons = screens.len() + 1;
                let mut button_x = rect[0];
                let mut button_y = rect[1];
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (rect[2] / (num_buttons as f32), rect[3]),
                    ListLayout::Vertical => (rect[2], rect[3] / (num_buttons as f32)),
                };

                canvas.bg_color = color_parse(&kbd_bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&kbd_fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = font_size;

                let keyboard = canvas.button(
                    button_x + 2.,
                    button_y + 2.,
                    button_w - 4.,
                    button_h - 4.,
                    KEYBOARD_NAME.into(),
                );
                keyboard.state = Some(ElemState {
                    button: Some(WatchButtonState {
                        pressed_at: Instant::now(),
                        overlay: Some(OverlaySelector::Name(KEYBOARD_NAME.into())),
                        mode: PointerMode::Left,
                    }),
                    ..Default::default()
                });
                keyboard.on_press = Some(overlay_button_dn);
                keyboard.on_release = Some(overlay_button_up);

                canvas.bg_color = color_parse(&scr_bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(&scr_fg_color).unwrap_or(FALLBACK_COLOR);

                for screen in screens.iter() {
                    button_x += match layout {
                        ListLayout::Horizontal => button_w,
                        ListLayout::Vertical => 0.,
                    };
                    button_y += match layout {
                        ListLayout::Horizontal => 0.,
                        ListLayout::Vertical => button_h,
                    };

                    let button = canvas.button(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        screen.state.name.clone(),
                    );
                    button.state = Some(ElemState {
                        button: Some(WatchButtonState {
                            pressed_at: Instant::now(),
                            overlay: Some(OverlaySelector::Id(screen.state.id)),
                            mode: PointerMode::Left,
                        }),
                        ..Default::default()
                    });

                    button.on_press = Some(overlay_button_dn);
                    button.on_release = Some(overlay_button_up);
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

#[derive(Default)]
struct ElemState {
    battery: Option<usize>,
    clock: Option<ClockState>,
    exec: Option<ExecState>,
    button: Option<WatchButtonState>,
}

struct ClockState {
    timezone: Option<Tz>,
    format: Arc<str>,
}

struct WatchButtonState {
    pressed_at: Instant,
    mode: PointerMode,
    overlay: Option<OverlaySelector>,
}

struct ExecState {
    last_exec: Instant,
    interval: f32,
    exec: Vec<Arc<str>>,
    child: Option<process::Child>,
}

fn battery_update(control: &mut Control<(), ElemState>, _: &mut (), app: &mut AppState) {
    let state = control.state.as_ref().unwrap();
    let device_idx = state.battery.unwrap();

    let device = app.input_state.devices.get(device_idx);

    let tags = ["", "H", "L", "R", "T"];

    let text = match device {
        Some(d) => d
            .soc
            .map(|soc| format!("{}{}", tags[d.role as usize], soc as u32))
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
    let state = control.state.as_mut().unwrap();
    let exec = state.exec.as_mut().unwrap();
    if let Some(child) = &mut exec.child {
        match child.try_wait() {
            Ok(Some(code)) => {
                if !code.success() {
                    log::error!("Child process exited with code: {}", code);
                }
                exec.child = None;
            }
            Ok(None) => {
                log::warn!("Unable to launch child process: previous child not exited yet");
                return;
            }
            Err(e) => {
                exec.child = None;
                log::error!("Error checking child process: {:?}", e);
            }
        }
    }
    let args = exec.exec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();
    match process::Command::new(args[0]).args(&args[1..]).spawn() {
        Ok(child) => {
            exec.child = Some(child);
        }
        Err(e) => {
            log::error!("Failed to spawn process {:?}: {:?}", args, e);
        }
    };
}

fn exec_label_update(control: &mut Control<(), ElemState>, _: &mut (), _: &mut AppState) {
    let state = control.state.as_mut().unwrap();
    let exec = state.exec.as_mut().unwrap();

    if let Some(mut child) = exec.child.take() {
        match child.try_wait() {
            Ok(Some(code)) => {
                if !code.success() {
                    log::error!("Child process exited with code: {}", code);
                } else {
                    if let Some(mut stdout) = child.stdout.take() {
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
                exec.child = Some(child);
                // not exited yet
                return;
            }
            Err(e) => {
                exec.child = None;
                log::error!("Error checking child process: {:?}", e);
                return;
            }
        }
    }

    if Instant::now()
        .saturating_duration_since(exec.last_exec)
        .as_secs_f32()
        > exec.interval
    {
        exec.last_exec = Instant::now();
        let args = exec.exec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();

        match process::Command::new(args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                exec.child = Some(child);
            }
            Err(e) => {
                log::error!("Failed to spawn process {:?}: {:?}", args, e);
            }
        };
    }
}

fn clock_update(control: &mut Control<(), ElemState>, _: &mut (), _: &mut AppState) {
    let state = control.state.as_mut().unwrap();
    let clock = state.clock.as_mut().unwrap();

    let fmt = clock.format.clone();

    if let Some(tz) = clock.timezone {
        let date = Local::now().with_timezone(&tz);
        control.set_text(&format!("{}", &date.format(&fmt)));
    } else {
        let date = Local::now();
        control.set_text(&format!("{}", &date.format(&fmt)));
    }
}

fn overlay_button_dn(
    control: &mut Control<(), ElemState>,
    _: &mut (),
    _: &mut AppState,
    mode: PointerMode,
) {
    let btn = control.state.as_mut().unwrap().button.as_mut().unwrap();
    btn.pressed_at = Instant::now();
    btn.mode = mode;
}

fn overlay_button_up(control: &mut Control<(), ElemState>, _: &mut (), app: &mut AppState) {
    let btn = control.state.as_mut().unwrap().button.as_mut().unwrap();
    let selector = btn.overlay.as_ref().unwrap().clone();
    if Instant::now()
        .saturating_duration_since(btn.pressed_at)
        .as_millis()
        < 2000
    {
        match btn.mode {
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
    OverlayList {
        rect: [f32; 4],
        font_size: isize,
        kbd_fg_color: Arc<str>,
        kbd_bg_color: Arc<str>,
        scr_fg_color: Arc<str>,
        scr_bg_color: Arc<str>,
        layout: ListLayout,
    },
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
