use std::{
    cell::RefCell,
    io::BufReader,
    process::{Child, ChildStdout},
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

use wgui::{
    event::{self, EventCallback, EventListenerKind},
    layout::Layout,
    parser::CustomAttribsInfoOwned,
    widget::EventResult,
};
use wlx_common::overlays::ToastTopic;

use crate::{
    backend::task::{OverlayTask, PlayspaceTask, TaskType},
    overlays::{
        mirror::{new_mirror, new_mirror_name},
        toast::Toast,
    },
    state::AppState,
    windowing::OverlaySelector,
    RUNNING,
};

#[cfg(feature = "wayvr")]
use crate::backend::wayvr::WayVRAction;

use super::helper::read_label_from_pipe;

pub const BUTTON_EVENTS: [(&str, EventListenerKind); 2] = [
    ("_press", EventListenerKind::MousePress),
    ("_release", EventListenerKind::MouseRelease),
];

pub(super) fn setup_custom_button<S: 'static>(
    layout: &mut Layout,
    attribs: &CustomAttribsInfoOwned,
    _app: &AppState,
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
            "::NewMirror" => Box::new(move |_common, _data, app, _| {
                let name = new_mirror_name();
                app.tasks.enqueue(TaskType::Overlay(OverlayTask::Create(
                    OverlaySelector::Name(name.clone()),
                    Box::new(move |app| Some(new_mirror(name, &app.session))),
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
                        format!("Fixing floor in {}", 5 - i).into(),
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
            #[allow(clippy::match_same_arms)]
            "::OscSend" => return,
            // shell
            _ => return,
        };

        let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
        log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
    }
}
struct ShellButtonMutableState {
    child: Option<Child>,
    reader: Option<BufReader<ChildStdout>>,
}

struct ShellButtonState {
    exec: String,
    mut_state: RefCell<ShellButtonMutableState>,
    carry_over: RefCell<Option<String>>,
}

// TODO
#[allow(clippy::missing_const_for_fn)]
fn shell_on_action(
    _state: &ShellButtonState,
    _common: &mut event::CallbackDataCommon,
    _data: &mut event::CallbackData,
) {
    //let mut mut_state = state.mut_state.borrow_mut();
}

fn shell_on_tick(
    state: &ShellButtonState,
    _common: &mut event::CallbackDataCommon,
    _data: &mut event::CallbackData,
) {
    let mut mut_state = state.mut_state.borrow_mut();

    if let Some(mut child) = mut_state.child.take() {
        match child.try_wait() {
            // not exited yet
            Ok(None) => {
                if let Some(_text) = mut_state.reader.as_mut().and_then(|r| {
                    read_label_from_pipe("child process", r, &mut state.carry_over.borrow_mut())
                }) {
                    //TODO update label
                }
                mut_state.child = Some(child);
            }
            // exited successfully
            Ok(Some(code)) if code.success() => {
                if let Some(_text) = mut_state.reader.as_mut().and_then(|r| {
                    read_label_from_pipe("child process", r, &mut state.carry_over.borrow_mut())
                }) {
                    //TODO update label
                }
                mut_state.child = None;
            }
            // exited with failure
            Ok(Some(code)) => {
                mut_state.child = None;
                log::warn!("Label process exited with code {code}");
            }
            // lost
            Err(_) => {
                mut_state.child = None;
                log::warn!("Label child process lost.");
            }
        }
    }
}
