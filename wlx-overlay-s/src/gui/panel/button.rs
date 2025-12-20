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
    event::{self, EventCallback, EventListenerKind},
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

pub const BUTTON_EVENTS: [(&str, EventListenerKind); 2] = [
    ("_press", EventListenerKind::MousePress),
    ("_release", EventListenerKind::MouseRelease),
];

pub(super) fn setup_custom_button<S: 'static>(
    layout: &mut Layout,
    attribs: &CustomAttribsInfoOwned,
    _app: &AppState,
    button: Rc<ComponentButton>,
) {
    for (name, kind) in &BUTTON_EVENTS {
        let Some(action) = attribs.get_value(name) else {
            continue;
        };

        let mut args = action.split_whitespace();
        let Some(command) = args.next() else {
            continue;
        };

        let callback: EventCallback<AppState, S> = match command {
            #[cfg(feature = "wayvr")]
            "::DashToggle" => Box::new(move |_common, _data, app, _| {
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
                Box::new(move |_common, _data, app, _| {
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

                Box::new(move |_common, _data, app, _| {
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
            "::EditToggle" => Box::new(move |_common, _data, app, _| {
                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::ToggleEditMode));
                Ok(EventResult::Consumed)
            }),
            #[cfg(feature = "wayland")]
            "::NewMirror" => Box::new(move |_common, _data, app, _| {
                let name = crate::overlays::mirror::new_mirror_name();
                app.tasks.enqueue(TaskType::Overlay(OverlayTask::Create(
                    OverlaySelector::Name(name.clone()),
                    Box::new(move |app| {
                        Some(crate::overlays::mirror::new_mirror(name, &app.session))
                    }),
                )));
                Ok(EventResult::Consumed)
            }),
            "::CleanupMirrors" => Box::new(move |_common, _data, app, _| {
                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::CleanupMirrors));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceReset" => Box::new(move |_common, _data, app, _| {
                app.tasks.enqueue(TaskType::Playspace(PlayspaceTask::Reset));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceRecenter" => Box::new(move |_common, _data, app, _| {
                app.tasks
                    .enqueue(TaskType::Playspace(PlayspaceTask::Recenter));
                Ok(EventResult::Consumed)
            }),
            "::PlayspaceFixFloor" => Box::new(move |_common, _data, app, _| {
                for i in 0..5 {
                    Toast::new(
                        ToastTopic::System,
                        format!("Fixing floor in {}", 5 - i),
                        "Touch your controller to the floor!".into(),
                    )
                    .with_timeout(1.)
                    .with_sound(true)
                    .submit_at(app, Instant::now() + Duration::from_secs(i));
                }
                app.tasks.enqueue_at(
                    TaskType::Playspace(PlayspaceTask::FixFloor),
                    Instant::now() + Duration::from_secs(5),
                );
                Ok(EventResult::Consumed)
            }),
            "::Shutdown" => Box::new(move |_common, _data, _app, _| {
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
                Box::new(move |_common, _data, app, _| {
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

                Box::new(move |_common, _data, _app, _| {
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

                Box::new(move |_common, _data, app, _| {
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
