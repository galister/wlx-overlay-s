use std::{
    f32::consts::PI,
    io::Cursor,
    ops::Add,
    process::{self, Child},
    sync::Arc,
    time::{Duration, Instant},
};

use glam::{Quat, Vec3A};
use rodio::{Decoder, Source};
use serde::Deserialize;

use crate::{
    backend::{
        common::{ColorChannel, OverlaySelector, SystemTask, TaskType},
        input::PointerMode,
        notifications::save_notifications,
        overlay::RelativeTo,
    },
    overlays::{
        toast::Toast,
        watch::{save_watch, WATCH_NAME},
    },
    state::AppState,
};

use super::{ExecArgs, ModularControl, ModularData};

#[derive(Deserialize, Clone, Copy)]
pub enum ViewAngleKind {
    /// The cosine of the angle at which the watch becomes fully transparent
    MinOpacity,
    /// The cosine of the angle at which the watch becomes fully opaque
    MaxOpacity,
}

#[derive(Deserialize, Clone, Copy)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Deserialize, Clone)]
pub enum SystemAction {
    ToggleNotificationSounds,
    ToggleNotifications,
    PlayspaceResetOffset,
    PlayspaceFixFloor,
    RecalculateExtent,
    PersistConfig,
}

#[derive(Deserialize, Clone)]
pub enum WatchAction {
    /// Hide the watch until Show/Hide binding is used
    Hide,
    /// Switch the watch to the opposite controller
    SwitchHands,
    /// Change the fade behavior of the watch
    ViewAngle {
        kind: ViewAngleKind,
        delta: f32,
    },
    Rotation {
        axis: Axis,
        delta: f32,
    },
    Position {
        axis: Axis,
        delta: f32,
    },
}

#[derive(Deserialize, Clone)]
pub enum OverlayAction {
    /// Reset the overlay to be in front of the HMD with its original scale
    Reset,
    /// Toggle the visibility of the overlay
    ToggleVisible,
    /// Toggle the ability to grab and recenter the overlay
    ToggleImmovable,
    /// Toggle the ability of the overlay to reacto to laser pointer
    ToggleInteraction,
    /// Change the opacity of the overlay
    Opacity { delta: f32 },
}

#[derive(Deserialize, Clone)]
pub enum WindowAction {
    /// Create a new mirror window, or show/hide an existing one
    ShowMirror,
    /// Create a new UI window, or show/hide an existing one
    ShowUi,
    /// Destroy a previously created window, if it exists
    Destroy,
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ButtonAction {
    Exec {
        command: ExecArgs,
        toast: Option<Arc<str>>,
    },
    Watch {
        action: WatchAction,
    },
    Overlay {
        target: OverlaySelector,
        action: OverlayAction,
    },
    Window {
        target: Arc<str>,
        action: WindowAction,
    },
    Toast {
        message: Arc<str>,
        body: Option<Arc<str>>,
        seconds: Option<f32>,
    },
    ColorAdjust {
        channel: ColorChannel,
        delta: f32,
    },
    System {
        action: SystemAction,
    },
}

pub(super) struct PressData {
    last_down: Instant,
    last_mode: PointerMode,
    child: Option<Child>,
}
impl Clone for PressData {
    fn clone(&self) -> Self {
        Self {
            last_down: self.last_down,
            last_mode: self.last_mode,
            child: None,
        }
    }
}
impl Default for PressData {
    fn default() -> Self {
        Self {
            last_down: Instant::now(),
            last_mode: PointerMode::Left,
            child: None,
        }
    }
}

#[derive(Deserialize, Default, Clone)]
pub struct ButtonData {
    #[serde(skip)]
    pub(super) press: PressData,

    pub(super) click_down: Option<Vec<ButtonAction>>,
    pub(super) click_up: Option<Vec<ButtonAction>>,
    pub(super) long_click_up: Option<Vec<ButtonAction>>,
    pub(super) right_down: Option<Vec<ButtonAction>>,
    pub(super) right_up: Option<Vec<ButtonAction>>,
    pub(super) long_right_up: Option<Vec<ButtonAction>>,
    pub(super) middle_down: Option<Vec<ButtonAction>>,
    pub(super) middle_up: Option<Vec<ButtonAction>>,
    pub(super) long_middle_up: Option<Vec<ButtonAction>>,
    pub(super) scroll_down: Option<Vec<ButtonAction>>,
    pub(super) scroll_up: Option<Vec<ButtonAction>>,
}

pub fn modular_button_init(button: &mut ModularControl, data: &ButtonData) {
    button.state = Some(ModularData::Button(Box::new(data.clone())));
    button.on_press = Some(modular_button_dn);
    button.on_release = Some(modular_button_up);
    button.on_scroll = Some(modular_button_scroll);
}

fn modular_button_dn(
    button: &mut ModularControl,
    _: &mut (),
    app: &mut AppState,
    mode: PointerMode,
) {
    // want panic
    let ModularData::Button(data) = button.state.as_mut().unwrap() else {
        panic!("modular_button_dn: button state is not Button");
    };

    data.press.last_down = Instant::now();
    data.press.last_mode = mode;

    let actions = match mode {
        PointerMode::Left => data.click_down.as_ref(),
        PointerMode::Right => data.right_down.as_ref(),
        PointerMode::Middle => data.middle_down.as_ref(),
        _ => None,
    };

    if let Some(actions) = actions {
        for action in actions {
            handle_action(action, &mut data.press, app);
        }
    }
}

fn modular_button_up(button: &mut ModularControl, _: &mut (), app: &mut AppState) {
    // want panic
    let ModularData::Button(data) = button.state.as_mut().unwrap() else {
        panic!("modular_button_up: button state is not Button");
    };

    let now = Instant::now();
    let duration = now - data.press.last_down;
    let long_press = duration.as_secs_f32() > app.session.config.long_press_duration;

    let actions = match data.press.last_mode {
        PointerMode::Left => {
            if long_press {
                data.long_click_up.as_ref()
            } else {
                data.click_up.as_ref()
            }
        }
        PointerMode::Right => {
            if long_press {
                data.long_right_up.as_ref()
            } else {
                data.right_up.as_ref()
            }
        }
        PointerMode::Middle => {
            if long_press {
                data.long_middle_up.as_ref()
            } else {
                data.middle_up.as_ref()
            }
        }
        _ => None,
    };

    if let Some(actions) = actions {
        for action in actions {
            handle_action(action, &mut data.press, app);
        }
    }
}

fn modular_button_scroll(button: &mut ModularControl, _: &mut (), app: &mut AppState, delta: f32) {
    // want panic
    let ModularData::Button(data) = button.state.as_mut().unwrap() else {
        panic!("modular_button_scroll: button state is not Button");
    };

    let actions = if delta < 0.0 {
        data.scroll_down.as_ref()
    } else {
        data.scroll_up.as_ref()
    };

    if let Some(actions) = actions {
        for action in actions {
            handle_action(action, &mut data.press, app);
        }
    }
}

fn handle_action(action: &ButtonAction, press: &mut PressData, app: &mut AppState) {
    match action {
        ButtonAction::Exec { command, toast } => run_exec(command, toast, press, app),
        ButtonAction::Watch { action } => run_watch(action, app),
        ButtonAction::Overlay { target, action } => run_overlay(target, action, app),
        ButtonAction::Window { target, action } => run_window(target, action, app),
        ButtonAction::Toast {
            message,
            body,
            seconds,
        } => {
            Toast::new(message.clone(), body.clone().unwrap_or_else(|| "".into()))
                .with_timeout(seconds.unwrap_or(5.))
                .submit(app);
        }
        ButtonAction::ColorAdjust { channel, delta } => {
            let channel = *channel;
            let delta = *delta;
            app.tasks
                .enqueue(TaskType::System(SystemTask::ColorGain(channel, delta)));
        }
        ButtonAction::System { action } => run_system(action, app),
    }
}

fn run_system(action: &SystemAction, app: &mut AppState) {
    match action {
        SystemAction::PlayspaceResetOffset => {
            app.tasks
                .enqueue(TaskType::System(SystemTask::ResetPlayspace));
        }
        SystemAction::PlayspaceFixFloor => {
            let now = Instant::now();
            let sec = Duration::from_secs(1);
            for i in 0..5 {
                let at = now.add(i * sec);
                let display = 5 - i;
                Toast::new(
                    format!("Fixing floor in {}", display).into(),
                    "Place either controller on the floor.".into(),
                )
                .with_timeout(1.0)
                .submit_at(app, at);
            }
            app.tasks
                .enqueue_at(TaskType::System(SystemTask::FixFloor), now.add(5 * sec));
        }
        SystemAction::RecalculateExtent => {
            todo!()
        }
        SystemAction::ToggleNotifications => {
            app.session.config.notifications_enabled = !app.session.config.notifications_enabled;
            Toast::new(
                format!(
                    "Notifications are {}.",
                    if app.session.config.notifications_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                )
                .into(),
                "".into(),
            )
            .submit(app);
        }
        SystemAction::ToggleNotificationSounds => {
            app.session.config.notifications_sound_enabled =
                !app.session.config.notifications_sound_enabled;
            Toast::new(
                format!(
                    "Notification sounds are {}.",
                    if app.session.config.notifications_sound_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                )
                .into(),
                "".into(),
            )
            .submit(app);
        }
        SystemAction::PersistConfig => {
            if let Err(e) = save_watch(app) {
                log::error!("Failed to save watch config: {:?}", e);
            };
            if let Err(e) = save_notifications(app) {
                log::error!("Failed to save notifications config: {:?}", e);
            }
        }
    }
}

fn run_exec(args: &ExecArgs, toast: &Option<Arc<str>>, press: &mut PressData, app: &mut AppState) {
    if let Some(proc) = press.child.as_mut() {
        match proc.try_wait() {
            Ok(Some(code)) => {
                if !code.success() {
                    log::error!("Child process exited with code: {}", code);
                }
                press.child = None;
            }
            Ok(None) => {
                log::warn!("Unable to launch child process: previous child not exited yet");
                return;
            }
            Err(e) => {
                press.child = None;
                log::error!("Error checking child process: {:?}", e);
            }
        }
    }
    let args = args.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();
    match process::Command::new(args[0]).args(&args[1..]).spawn() {
        Ok(proc) => {
            press.child = Some(proc);
            if let Some(toast) = toast.as_ref() {
                Toast::new(toast.clone(), "".into()).submit(app);
            }
        }
        Err(e) => {
            log::error!("Failed to spawn process {:?}: {:?}", args, e);
        }
    };
}

fn run_watch(data: &WatchAction, app: &mut AppState) {
    match data {
        WatchAction::Hide => {
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(|app, o| {
                    if o.saved_scale.is_none() {
                        o.saved_scale = Some(o.spawn_scale);
                        o.want_visible = false;
                        o.saved_scale = Some(0.0);
                        Toast::new(
                            "Watch hidden".into(),
                            "Use show/hide binding to restore.".into(),
                        )
                        .with_timeout(3.)
                        .submit(app);
                    } else {
                        o.want_visible = true;
                        o.saved_scale = None;
                        Toast::new("Watch restored".into(), "".into()).submit(app);
                    }
                }),
            ));
            audio_thump(app);
        }
        WatchAction::SwitchHands => {
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(|app, o| {
                    if let RelativeTo::Hand(0) = o.relative_to {
                        o.relative_to = RelativeTo::Hand(1);
                        o.spawn_rotation = Quat::from_slice(&app.session.config.watch_rot)
                            * Quat::from_rotation_x(PI)
                            * Quat::from_rotation_z(PI);
                        o.spawn_point = Vec3A::from_slice(&app.session.config.watch_pos);
                        o.spawn_point.x *= -1.;
                    } else {
                        o.relative_to = RelativeTo::Hand(0);
                        o.spawn_rotation = Quat::from_slice(&app.session.config.watch_rot);
                        o.spawn_point = Vec3A::from_slice(&app.session.config.watch_pos);
                    }
                    o.dirty = true;
                    Toast::new("Watch switched".into(), "Check your other hand".into())
                        .with_timeout(3.)
                        .submit(app);
                }),
            ));
            audio_thump(app);
        }
        WatchAction::ViewAngle { kind, delta } => match kind {
            ViewAngleKind::MinOpacity => {
                let diff = (app.session.config.watch_view_angle_max
                    - app.session.config.watch_view_angle_min)
                    + delta;

                app.session.config.watch_view_angle_min = (app.session.config.watch_view_angle_max
                    - diff)
                    .clamp(0.0, app.session.config.watch_view_angle_max - 0.05);
            }
            ViewAngleKind::MaxOpacity => {
                let diff = app.session.config.watch_view_angle_max
                    - app.session.config.watch_view_angle_min;

                app.session.config.watch_view_angle_max =
                    (app.session.config.watch_view_angle_max + delta).clamp(0.05, 1.0);

                app.session.config.watch_view_angle_min = (app.session.config.watch_view_angle_max
                    - diff)
                    .clamp(0.0, app.session.config.watch_view_angle_max - 0.05);
            }
        },
        WatchAction::Rotation { axis, delta } => {
            let rot = match axis {
                Axis::X => Quat::from_rotation_x(delta.to_radians()),
                Axis::Y => Quat::from_rotation_y(delta.to_radians()),
                Axis::Z => Quat::from_rotation_z(delta.to_radians()),
            };
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(move |app, o| {
                    o.spawn_rotation *= rot;
                    app.session.config.watch_rot = o.spawn_rotation.into();
                    o.dirty = true;
                }),
            ));
        }
        WatchAction::Position { axis, delta } => {
            let delta = *delta;
            let axis = match axis {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            };
            app.tasks.enqueue(TaskType::Overlay(
                OverlaySelector::Name(WATCH_NAME.into()),
                Box::new(move |app, o| {
                    o.spawn_point[axis] += delta;
                    app.session.config.watch_pos = o.spawn_point.into();
                    o.dirty = true;
                }),
            ));
        }
    }
}

fn run_overlay(overlay: &OverlaySelector, action: &OverlayAction, app: &mut AppState) {
    match action {
        OverlayAction::Reset => {
            app.tasks.enqueue(TaskType::Overlay(
                overlay.clone(),
                Box::new(|app, o| {
                    o.reset(app, true);
                    Toast::new(format!("{} has been reset!", o.name).into(), "".into()).submit(app);
                }),
            ));
        }
        OverlayAction::ToggleVisible => {
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
        OverlayAction::ToggleImmovable => {
            app.tasks.enqueue(TaskType::Overlay(
                overlay.clone(),
                Box::new(|app, o| {
                    o.recenter = !o.recenter;
                    o.grabbable = o.recenter;
                    o.show_hide = o.recenter;
                    if !o.recenter {
                        Toast::new(
                            format!("{} is now locked in place!", o.name).into(),
                            "".into(),
                        )
                        .submit(app);
                    } else {
                        Toast::new(format!("{} is now unlocked!", o.name).into(), "".into())
                            .submit(app);
                    }
                }),
            ));
            audio_thump(app);
        }
        OverlayAction::ToggleInteraction => {
            app.tasks.enqueue(TaskType::Overlay(
                overlay.clone(),
                Box::new(|app, o| {
                    o.interactable = !o.interactable;
                    if !o.interactable {
                        Toast::new(
                            format!("{} is now non-interactable!", o.name).into(),
                            "".into(),
                        )
                        .submit(app);
                    } else {
                        Toast::new(format!("{} is now interactable!", o.name).into(), "".into())
                            .submit(app);
                    }
                }),
            ));
            audio_thump(app);
        }
        OverlayAction::Opacity { delta } => {
            let delta = *delta;
            app.tasks.enqueue(TaskType::Overlay(
                overlay.clone(),
                Box::new(move |_, o| {
                    o.alpha = (o.alpha + delta).clamp(0.1, 1.0);
                    o.dirty = true;
                    log::debug!("{}: alpha {}", o.name, o.alpha);
                }),
            ));
        }
    }
}

fn run_window(window: &Arc<str>, action: &WindowAction, app: &mut AppState) {
    use crate::overlays::custom;

    match action {
        WindowAction::ShowMirror => {
            #[cfg(feature = "wayland")]
            app.tasks.enqueue(TaskType::CreateOverlay(
                OverlaySelector::Name(window.clone()),
                Box::new({
                    let name = window.clone();
                    move |app| {
                        Toast::new("Check your desktop for popup.".into(), "".into())
                            .with_sound(true)
                            .submit(app);
                        crate::overlays::mirror::new_mirror(name.clone(), false, &app.session)
                    }
                }),
            ));
            #[cfg(not(feature = "wayland"))]
            log::warn!("Mirror not available without Wayland feature.");
        }
        WindowAction::ShowUi => {
            app.tasks.enqueue(TaskType::CreateOverlay(
                OverlaySelector::Name(window.clone()),
                Box::new({
                    let name = window.clone();
                    move |app| custom::create_custom(app, name)
                }),
            ));
        }
        WindowAction::Destroy => {
            app.tasks
                .enqueue(TaskType::DropOverlay(OverlaySelector::Name(window.clone())));
        }
    }
}

fn audio_thump(app: &mut AppState) {
    if let Some(handle) = app.audio.get_handle() {
        let wav = include_bytes!("../../res/380885.wav");
        let cursor = Cursor::new(wav);
        let source = Decoder::new_wav(cursor).unwrap();
        let _ = handle.play_raw(source.convert_samples());
    }
}
