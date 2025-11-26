use std::{
    cell::RefCell,
    io::BufReader,
    process::{Child, ChildStdout},
    sync::Arc,
};

use wgui::{
    event::{self, EventCallback, EventListenerKind},
    layout::Layout,
    parser::CustomAttribsInfoOwned,
    widget::EventResult,
};

use crate::{backend::task::ManagerTask, state::AppState, windowing::OverlaySelector};

#[cfg(feature = "wayvr")]
use crate::backend::{task::TaskType, wayvr::WayVRAction};

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
            "::DashToggle" => Box::new(move |_common, _data, app, _| {
                #[cfg(feature = "wayvr")]
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
                        .enqueue(TaskType::Manager(ManagerTask::ToggleSet(set_idx)));
                    Ok(EventResult::Consumed)
                })
            }
            "::OverlayToggle" => {
                let Some(arg): Option<Arc<str>> = args.next().map(Into::into) else {
                    log::error!("{command} has missing arguments");
                    return;
                };

                Box::new(move |_common, _data, app, _| {
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Name(arg.clone()),
                        Box::new(move |app, owc| {
                            if owc.active_state.is_none() {
                                owc.activate(app);
                            } else {
                                owc.deactivate();
                            }
                        }),
                    ));
                    Ok(EventResult::Consumed)
                })
            }
            "::EditToggle" => Box::new(move |_common, _data, app, _| {
                app.tasks
                    .enqueue(TaskType::Manager(ManagerTask::ToggleEditMode));
                Ok(EventResult::Consumed)
            }),
            "::WatchHide" => todo!(),
            "::WatchSwapHand" => todo!(),
            // TODO
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
