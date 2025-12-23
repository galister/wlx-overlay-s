use std::{
    cell::RefCell,
    process::{Command, Stdio},
    rc::Rc,
    str::FromStr,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use anyhow::Context;
use wgui::{
    components::button::ComponentButton,
    event::{
        self, CallbackData, CallbackMetadata, EventCallback, EventListenerKind, MouseButtonIndex,
    },
    i18n::Translation,
    layout::Layout,
    parser::CustomAttribsInfoOwned,
    widget::EventResult,
};
use wlx_common::overlays::ToastTopic;

use crate::{
    RUNNING,
    backend::task::{OverlayTask, PlayspaceTask, TaskType},
    gui::panel::helper::PipeReaderThread,
    overlays::toast::Toast,
    state::AppState,
    subsystem::hid::VirtualKey,
    windowing::OverlaySelector,
};

#[cfg(feature = "wayvr")]
use crate::backend::wayvr::WayVRAction;

pub const BUTTON_EVENTS: [(
    &str,
    EventListenerKind,
    fn(&mut CallbackData) -> bool,
    fn(&ComponentButton, &AppState) -> bool,
); 16] = [
    (
        "_press",
        EventListenerKind::MousePress,
        button_any,
        short_duration,
    ),
    (
        "_release",
        EventListenerKind::MouseRelease,
        button_any,
        ignore_duration,
    ),
    (
        "_press_left",
        EventListenerKind::MousePress,
        button_left,
        ignore_duration,
    ),
    (
        "_release_left",
        EventListenerKind::MouseRelease,
        button_left,
        ignore_duration,
    ),
    (
        "_press_right",
        EventListenerKind::MousePress,
        button_right,
        ignore_duration,
    ),
    (
        "_release_right",
        EventListenerKind::MouseRelease,
        button_right,
        ignore_duration,
    ),
    (
        "_press_middle",
        EventListenerKind::MousePress,
        button_middle,
        ignore_duration,
    ),
    (
        "_release_middle",
        EventListenerKind::MouseRelease,
        button_middle,
        ignore_duration,
    ),
    (
        "_short_release",
        EventListenerKind::MouseRelease,
        button_any,
        short_duration,
    ),
    (
        "_short_release_left",
        EventListenerKind::MouseRelease,
        button_left,
        short_duration,
    ),
    (
        "_short_release_right",
        EventListenerKind::MouseRelease,
        button_right,
        short_duration,
    ),
    (
        "_short_release_middle",
        EventListenerKind::MouseRelease,
        button_middle,
        short_duration,
    ),
    (
        "_long_release",
        EventListenerKind::MouseRelease,
        button_any,
        long_duration,
    ),
    (
        "_long_release_left",
        EventListenerKind::MouseRelease,
        button_left,
        long_duration,
    ),
    (
        "_long_release_right",
        EventListenerKind::MouseRelease,
        button_right,
        long_duration,
    ),
    (
        "_long_release_middle",
        EventListenerKind::MouseRelease,
        button_middle,
        long_duration,
    ),
];

fn button_any(_: &mut CallbackData) -> bool {
    true
}

fn button_left(data: &mut CallbackData) -> bool {
    if let CallbackMetadata::MouseButton(b) = data.metadata
        && let MouseButtonIndex::Left = b.index
    {
        true
    } else {
        false
    }
}
fn button_right(data: &mut CallbackData) -> bool {
    if let CallbackMetadata::MouseButton(b) = data.metadata
        && let MouseButtonIndex::Right = b.index
    {
        true
    } else {
        false
    }
}
fn button_middle(data: &mut CallbackData) -> bool {
    if let CallbackMetadata::MouseButton(b) = data.metadata
        && let MouseButtonIndex::Middle = b.index
    {
        true
    } else {
        false
    }
}

fn ignore_duration(_btn: &ComponentButton, _app: &AppState) -> bool {
    true
}
fn long_duration(btn: &ComponentButton, app: &AppState) -> bool {
    btn.get_time_since_last_pressed().as_secs_f32() > app.session.config.long_press_duration
}
fn short_duration(btn: &ComponentButton, app: &AppState) -> bool {
    btn.get_time_since_last_pressed().as_secs_f32() < app.session.config.long_press_duration
}

pub(super) fn setup_custom_button<S: 'static>(
    layout: &mut Layout,
    attribs: &CustomAttribsInfoOwned,
    _app: &AppState,
    button: Rc<ComponentButton>,
) {
    for (name, kind, test_button, test_duration) in &BUTTON_EVENTS {
        let Some(action) = attribs.get_value(name) else {
            continue;
        };

        let mut args = action.split_whitespace();
        let Some(command) = args.next() else {
            continue;
        };

        let button = button.clone();

        let callback: EventCallback<AppState, S> = match command {
            #[cfg(feature = "wayvr")]
            "::DashToggle" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                app.tasks
                    .enqueue(TaskType::WayVR(WayVRAction::ToggleDashboard));
                Ok(EventResult::Consumed)
            }),
            "::SetToggle" => {
                let arg = args.next().unwrap_or_default();
                let Ok(set_idx) = arg.parse() else {
                    log::error!("{command} has invalid argument: \"{arg}\"");
                    return;
                };
                Box::new(move |_common, data, app, _| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    app.tasks
                        .enqueue(TaskType::Overlay(OverlayTask::ToggleSet(set_idx)));
                    Ok(EventResult::Consumed)
                })
            }
            "::OverlayToggle" => {
                let Some(arg): Option<Arc<str>> = args.next().map(Into::into) else {
                    log::error!("{command} has missing arguments");
                    return;
                };

                Box::new(move |_common, data, app, _| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
                        OverlaySelector::Name(arg.clone()),
                        Box::new(move |app, owc| {
                            if owc.active_state.is_none() {
                                owc.activate(app);
                            } else {
                                owc.deactivate();
                            }
                        }),
                    )));
                    Ok(EventResult::Consumed)
                })
            }
            "::EditToggle" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::ToggleEditMode));
                Ok(EventResult::Consumed)
            }),
            #[cfg(feature = "wayland")]
            "::NewMirror" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                let name = crate::overlays::screen::mirror::new_mirror_name();
                app.tasks.enqueue(TaskType::Overlay(OverlayTask::Create(
                    OverlaySelector::Name(name.clone()),
                    Box::new(move |app| {
                        Some(crate::overlays::screen::mirror::new_mirror(
                            name,
                            &app.session,
                        ))
                    }),
                )));
                Ok(EventResult::Consumed)
            }),
            "::CleanupMirrors" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::CleanupMirrors));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceReset" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                app.tasks.enqueue(TaskType::Playspace(PlayspaceTask::Reset));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceRecenter" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                app.tasks
                    .enqueue(TaskType::Playspace(PlayspaceTask::Recenter));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceFixFloor" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                Toast::new(
                    ToastTopic::System,
                    "TOAST.FIXING_FLOOR".into(),
                    "TOAST.ONE_CONTROLLER_ON_FLOOR".into(),
                )
                .with_timeout(5.)
                .with_sound(true)
                .submit(app);

                app.tasks.enqueue_at(
                    TaskType::Playspace(PlayspaceTask::FixFloor),
                    Instant::now() + Duration::from_secs(5),
                );
                Ok(EventResult::Consumed)
            }),
            "::Shutdown" => Box::new(move |_common, data, app, _| {
                if !test_button(data) || !test_duration(&button, app) {
                    return Ok(EventResult::Pass);
                }

                RUNNING.store(false, Ordering::Relaxed);
                Ok(EventResult::Consumed)
            }),
            "::SendKey" => {
                let Some(key) = args.next().and_then(|s| VirtualKey::from_str(s).ok()) else {
                    log::error!("{command} has bad/missing arguments");
                    return;
                };
                let Some(down) = args.next().and_then(|s| match s.to_lowercase().as_str() {
                    "down" => Some(true),
                    "up" => Some(false),
                    _ => None,
                }) else {
                    log::error!("{command} has bad/missing arguments");
                    return;
                };
                Box::new(move |_common, data, app, _| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    app.hid_provider.send_key_routed(key, down);
                    Ok(EventResult::Consumed)
                })
            }
            "::ShellExec" => {
                let state = Rc::new(ShellButtonState {
                    button: button.clone(),
                    exec: args.fold(String::new(), |c, n| c + " " + n),
                    mut_state: RefCell::new(ShellButtonMutableState::default()),
                    carry_over: RefCell::new(None),
                });

                let piped = attribs.get_value("_update_label").is_some_and(|s| s == "1");

                layout.add_event_listener::<AppState, S>(
                    attribs.widget_id,
                    EventListenerKind::InternalStateChange,
                    Box::new({
                        let state = state.clone();
                        move |common, _data, _, _| {
                            shell_on_tick(&state, common, piped);
                            Ok(EventResult::Pass)
                        }
                    }),
                );

                Box::new(move |_common, data, app, _| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    let _ = shell_on_action(&state).inspect_err(|e| log::error!("{e:?}"));
                    Ok(EventResult::Consumed)
                })
            }
            #[cfg(feature = "osc")]
            "::OscSend" => {
                use crate::subsystem::osc::parse_osc_value;

                let Some(address) = args.next().map(std::string::ToString::to_string) else {
                    log::error!("{command} has missing arguments");
                    return;
                };

                let mut osc_args = vec![];
                for arg in args {
                    let Ok(osc_arg) = parse_osc_value(arg)
                        .inspect_err(|e| log::error!("Could not parse OSC value '{arg}': {e:?}"))
                    else {
                        return;
                    };
                    osc_args.push(osc_arg);
                }

                Box::new(move |_common, data, app, _| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    let Some(sender) = app.osc_sender.as_mut() else {
                        log::error!("OscSend: sender is not available.");
                        return Ok(EventResult::Consumed);
                    };

                    let _ = sender
                        .send_message(address.clone(), osc_args.clone())
                        .inspect_err(|e| log::error!("OscSend: Could not send message: {e:?}"));

                    Ok(EventResult::Consumed)
                })
            }
            // shell
            _ => return,
        };

        let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
        log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
    }
}

#[derive(Default)]
struct ShellButtonMutableState {
    reader: Option<PipeReaderThread>,
    pid: Option<u32>,
}

struct ShellButtonState {
    button: Rc<ComponentButton>,
    exec: String,
    mut_state: RefCell<ShellButtonMutableState>,
    carry_over: RefCell<Option<String>>,
}

fn shell_on_action(state: &ShellButtonState) -> anyhow::Result<()> {
    let mut mut_state = state.mut_state.borrow_mut();

    if mut_state.reader.as_ref().is_some_and(|r| !r.is_finished())
        && let Some(pid) = mut_state.pid.as_ref()
    {
        log::info!("ShellExec triggered while child is still running; sending SIGUSR1");
        let _ = Command::new("kill")
            .arg("-s")
            .arg("USR1")
            .arg(pid.to_string())
            .spawn()
            .unwrap()
            .wait();
        return Ok(());
    }

    let child = Command::new("sh")
        .arg("-c")
        .arg(&state.exec)
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run shell script: '{}'", &state.exec))?;

    mut_state.pid = Some(child.id());
    mut_state.reader = Some(PipeReaderThread::new_from_child(child));

    Ok(())
}

fn shell_on_tick(state: &ShellButtonState, common: &mut event::CallbackDataCommon, piped: bool) {
    let mut mut_state = state.mut_state.borrow_mut();

    let Some(reader) = mut_state.reader.as_mut() else {
        return;
    };

    if piped && let Some(text) = reader.get_last_line() {
        state
            .button
            .set_text(common, Translation::from_raw_text(&text));
    }

    if reader.is_finished() {
        mut_state.reader = None;
    }
}
