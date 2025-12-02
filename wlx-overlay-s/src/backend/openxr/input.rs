use std::{
    array::from_fn,
    mem::transmute,
    time::{Duration, Instant},
};

use glam::{Affine3A, Quat, Vec3, bool};
use libmonado as mnd;
use openxr::{self as xr, Quaternionf, Vector2f, Vector3f};
use serde::{Deserialize, Serialize};

use crate::{
    backend::input::{Haptics, Pointer, TrackedDevice, TrackedDeviceRole},
    config_io,
    state::{AppSession, AppState},
};

use super::{XrState, helpers::posef_to_transform};

static CLICK_TIMES: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_millis(500),
    Duration::from_millis(750),
];

pub(super) struct OpenXrInputSource {
    action_set: xr::ActionSet,
    hands: [OpenXrHand; 2],
}

pub(super) struct OpenXrHand {
    source: OpenXrHandSource,
    space: xr::Space,
}

pub struct MultiClickHandler<const COUNT: usize> {
    name: String,
    action_f32: xr::Action<f32>,
    action_bool: xr::Action<bool>,
    previous: [Instant; COUNT],
    held_active: bool,
    held_inactive: bool,
}

pub struct ClickThresholdHandler {
    handler: MultiClickHandler<0>,
    threshold: f32,
    threshold_release: f32,
}

impl<const COUNT: usize> MultiClickHandler<COUNT> {
    fn new(action_set: &xr::ActionSet, action_name: &str, side: &str) -> anyhow::Result<Self> {
        let name = format!("{side}_{COUNT}-{action_name}");
        let name_f32 = format!("{}_value", &name);

        let action_bool = action_set.create_action::<bool>(&name, &name, &[])?;
        let action_f32 = action_set.create_action::<f32>(&name_f32, &name_f32, &[])?;

        Ok(Self {
            name,
            action_f32,
            action_bool,
            previous: from_fn(|_| Instant::now()),
            held_active: false,
            held_inactive: false,
        })
    }
    fn check<G>(&mut self, session: &xr::Session<G>, threshold: f32) -> anyhow::Result<bool> {
        let res = self.action_bool.state(session, xr::Path::NULL)?;
        let mut state = res.is_active && res.current_state;

        if !state {
            let res = self.action_f32.state(session, xr::Path::NULL)?;
            state = res.is_active && res.current_state > threshold;
        }

        if !state {
            self.held_active = false;
            self.held_inactive = false;
            return Ok(false);
        }

        if self.held_active {
            return Ok(true);
        }

        if self.held_inactive {
            return Ok(false);
        }

        let passed = self
            .previous
            .iter()
            .all(|instant| instant.elapsed() < CLICK_TIMES[COUNT]);

        if passed {
            log::trace!("{}: passed", self.name);
            self.held_active = true;
            self.held_inactive = false;

            // reset to no prior clicks
            let long_ago = Instant::now().checked_sub(Duration::from_secs(10)).unwrap();
            self.previous
                .iter_mut()
                .for_each(|instant| *instant = long_ago);
        } else if COUNT > 0 {
            log::trace!("{}: rotate", self.name);
            self.previous.rotate_right(1);
            self.previous[0] = Instant::now();
            self.held_inactive = true;
        }

        Ok(passed)
    }
}

impl ClickThresholdHandler {
    pub fn new(action_set: &xr::ActionSet, action_name: &str, side: &str) -> anyhow::Result<Self> {
        Ok(Self {
            handler: MultiClickHandler::new(action_set, &format!("{action_name}_threshold"), side)?,
            threshold: 0.0,
            threshold_release: 0.0,
        })
    }

    pub const fn set_thresholds(&mut self, thresholds: [f32; 2]) {
        self.threshold = thresholds[0];
        self.threshold_release = thresholds[1];
    }

    pub fn check<G>(&mut self, session: &xr::Session<G>, before: bool) -> anyhow::Result<bool> {
        let threshold = if before {
            self.threshold_release
        } else {
            self.threshold
        };

        self.handler.check(session, threshold)
    }
}

pub struct CustomClickAction {
    single_threshold: ClickThresholdHandler,
    single: MultiClickHandler<0>,
    double: MultiClickHandler<1>,
    triple: MultiClickHandler<2>,
}

impl CustomClickAction {
    pub fn new(action_set: &xr::ActionSet, name: &str, side: &str) -> anyhow::Result<Self> {
        let single_threshold = ClickThresholdHandler::new(action_set, name, side)?;
        let single = MultiClickHandler::new(action_set, name, side)?;
        let double = MultiClickHandler::new(action_set, name, side)?;
        let triple = MultiClickHandler::new(action_set, name, side)?;

        Ok(Self {
            single_threshold,
            single,
            double,
            triple,
        })
    }
    pub fn state(
        &mut self,
        before: bool,
        state: &XrState,
        session: &AppSession,
    ) -> anyhow::Result<bool> {
        let threshold = if before {
            session.config.xr_click_sensitivity_release
        } else {
            session.config.xr_click_sensitivity
        };

        Ok(self.single.check(&state.session, threshold)?
            || self.double.check(&state.session, threshold)?
            || self.triple.check(&state.session, threshold)?
            || self.single_threshold.check(&state.session, before)?)
    }
}

pub(super) struct OpenXrHandSource {
    pose: xr::Action<xr::Posef>,
    click: CustomClickAction,
    grab: CustomClickAction,
    alt_click: CustomClickAction,
    show_hide: CustomClickAction,
    toggle_dashboard: CustomClickAction,
    space_drag: CustomClickAction,
    space_rotate: CustomClickAction,
    space_reset: CustomClickAction,
    modifier_right: CustomClickAction,
    modifier_middle: CustomClickAction,
    move_mouse: CustomClickAction,
    scroll: xr::Action<Vector2f>,
    haptics: xr::Action<xr::Haptic>,
}

impl OpenXrInputSource {
    pub fn new(xr: &XrState) -> anyhow::Result<Self> {
        let mut action_set =
            xr.session
                .instance()
                .create_action_set("wlx-overlay-s", "WlxOverlay-S Actions", 0)?;

        let mut left_source = OpenXrHandSource::new(&mut action_set, "left")?;
        let mut right_source = OpenXrHandSource::new(&mut action_set, "right")?;

        suggest_bindings(&xr.instance, &mut left_source, &mut right_source);

        xr.session.attach_action_sets(&[&action_set])?;

        Ok(Self {
            action_set,
            hands: [
                OpenXrHand::new(xr, left_source)?,
                OpenXrHand::new(xr, right_source)?,
            ],
        })
    }

    pub fn haptics(&self, xr: &XrState, hand: usize, haptics: &Haptics) {
        let action = &self.hands[hand].source.haptics;

        let duration_nanos = f64::from(haptics.duration) * 1_000_000_000.0;

        let _ = action.apply_feedback(
            &xr.session,
            xr::Path::NULL,
            &xr::HapticVibration::new()
                .amplitude(haptics.intensity)
                .frequency(haptics.frequency)
                .duration(xr::Duration::from_nanos(duration_nanos as _)),
        );
    }

    pub fn update(&mut self, xr: &XrState, state: &mut AppState) -> anyhow::Result<()> {
        xr.session.sync_actions(&[(&self.action_set).into()])?;

        let loc = xr.view.locate(&xr.stage, xr.predicted_display_time)?;
        let hmd = posef_to_transform(&loc.pose);
        if loc
            .location_flags
            .contains(xr::SpaceLocationFlags::ORIENTATION_VALID)
        {
            state.input_state.hmd.matrix3 = hmd.matrix3;
        }

        if loc
            .location_flags
            .contains(xr::SpaceLocationFlags::POSITION_VALID)
        {
            state.input_state.hmd.translation = hmd.translation;
        }

        for i in 0..2 {
            self.hands[i].update(&mut state.input_state.pointers[i], xr, &state.session)?;
        }
        Ok(())
    }

    fn update_device_battery_status(
        device: &mut mnd::Device,
        role: TrackedDeviceRole,
        app: &mut AppState,
    ) {
        if let Ok(status) = device.battery_status()
            && status.present
        {
            app.input_state.devices.push(TrackedDevice {
                soc: Some(status.charge),
                charging: status.charging,
                role,
            });
            log::debug!(
                "Device {} role {:#?}: {:.0}% (charging {})",
                device.index,
                role,
                status.charge * 100.0f32,
                status.charging
            );
        }
    }

    pub fn update_devices(app: &mut AppState, monado: &mut mnd::Monado) -> bool {
        let old_len = app.input_state.devices.len();
        app.input_state.devices.clear();

        let roles = [
            (mnd::DeviceRole::Head, TrackedDeviceRole::Hmd),
            (mnd::DeviceRole::Eyes, TrackedDeviceRole::None),
            (mnd::DeviceRole::Left, TrackedDeviceRole::LeftHand),
            (mnd::DeviceRole::Right, TrackedDeviceRole::RightHand),
            (mnd::DeviceRole::Gamepad, TrackedDeviceRole::None),
            (
                mnd::DeviceRole::HandTrackingLeft,
                TrackedDeviceRole::LeftHand,
            ),
            (
                mnd::DeviceRole::HandTrackingRight,
                TrackedDeviceRole::RightHand,
            ),
        ];
        let mut seen = Vec::<u32>::with_capacity(32);
        for (mnd_role, wlx_role) in roles {
            let device = monado.device_from_role(mnd_role);
            if let Ok(mut device) = device
                && !seen.contains(&device.index)
            {
                seen.push(device.index);
                Self::update_device_battery_status(&mut device, wlx_role, app);
            }
        }
        if let Ok(devices) = monado.devices() {
            for mut device in devices {
                if !seen.contains(&device.index) {
                    let role = if device.name_id >= 4 && device.name_id <= 8 {
                        TrackedDeviceRole::Tracker
                    } else {
                        TrackedDeviceRole::None
                    };
                    Self::update_device_battery_status(&mut device, role, app);
                }
            }
        }

        app.input_state.devices.sort_by(|a, b| {
            u8::from(a.soc.is_none())
                .cmp(&u8::from(b.soc.is_none()))
                .then((a.role as u8).cmp(&(b.role as u8)))
                .then(a.soc.unwrap_or(999.).total_cmp(&b.soc.unwrap_or(999.)))
        });

        old_len != app.input_state.devices.len()
    }
}

impl OpenXrHand {
    pub(super) fn new(xr: &XrState, source: OpenXrHandSource) -> Result<Self, xr::sys::Result> {
        let space = source
            .pose
            .create_space(&xr.session, xr::Path::NULL, xr::Posef::IDENTITY)?;

        Ok(Self { source, space })
    }

    pub(super) fn update(
        &mut self,
        pointer: &mut Pointer,
        xr: &XrState,
        session: &AppSession,
    ) -> anyhow::Result<()> {
        let location = self.space.locate(&xr.stage, xr.predicted_display_time)?;
        if location
            .location_flags
            .contains(xr::SpaceLocationFlags::ORIENTATION_VALID)
        {
            let (cur_quat, cur_pos) = (Quat::from_affine3(&pointer.pose), pointer.pose.translation);

            let (new_quat, new_pos) = unsafe {
                (
                    transmute::<Quaternionf, Quat>(location.pose.orientation),
                    transmute::<Vector3f, Vec3>(location.pose.position),
                )
            };
            let lerp_factor =
                (1.0 / (xr.fps / 100.0) * session.config.pointer_lerp_factor).clamp(0.1, 1.0);
            pointer.raw_pose = Affine3A::from_rotation_translation(new_quat, new_pos);
            pointer.pose = Affine3A::from_rotation_translation(
                cur_quat.lerp(new_quat, lerp_factor),
                cur_pos.lerp(new_pos.into(), lerp_factor).into(),
            );
        }

        pointer.now.click = self.source.click.state(pointer.before.click, xr, session)?;

        pointer.now.grab = self.source.grab.state(pointer.before.grab, xr, session)?;

        let scroll = self
            .source
            .scroll
            .state(&xr.session, xr::Path::NULL)?
            .current_state;

        pointer.now.scroll_x = scroll.x;
        pointer.now.scroll_y = scroll.y;

        pointer.now.alt_click =
            self.source
                .alt_click
                .state(pointer.before.alt_click, xr, session)?;

        pointer.now.show_hide =
            self.source
                .show_hide
                .state(pointer.before.show_hide, xr, session)?;

        pointer.now.click_modifier_right =
            self.source
                .modifier_right
                .state(pointer.before.click_modifier_right, xr, session)?;

        pointer.now.toggle_dashboard =
            self.source
                .toggle_dashboard
                .state(pointer.before.toggle_dashboard, xr, session)?;

        pointer.now.click_modifier_middle =
            self.source
                .modifier_middle
                .state(pointer.before.click_modifier_middle, xr, session)?;

        pointer.now.move_mouse =
            self.source
                .move_mouse
                .state(pointer.before.move_mouse, xr, session)?;

        pointer.now.space_drag =
            self.source
                .space_drag
                .state(pointer.before.space_drag, xr, session)?;

        pointer.now.space_rotate =
            self.source
                .space_rotate
                .state(pointer.before.space_rotate, xr, session)?;

        pointer.now.space_reset =
            self.source
                .space_reset
                .state(pointer.before.space_reset, xr, session)?;

        Ok(())
    }
}

// supported action types: Haptic, Posef, Vector2f, f32, bool
impl OpenXrHandSource {
    pub(super) fn new(action_set: &mut xr::ActionSet, side: &str) -> anyhow::Result<Self> {
        let action_pose = action_set.create_action::<xr::Posef>(
            &format!("{side}_hand"),
            &format!("{side} hand pose"),
            &[],
        )?;

        let action_scroll = action_set.create_action::<Vector2f>(
            &format!("{side}_scroll"),
            &format!("{side} hand scroll"),
            &[],
        )?;
        let action_haptics = action_set.create_action::<xr::Haptic>(
            &format!("{side}_haptics"),
            &format!("{side} hand haptics"),
            &[],
        )?;

        Ok(Self {
            pose: action_pose,
            click: CustomClickAction::new(action_set, "click", side)?,
            grab: CustomClickAction::new(action_set, "grab", side)?,
            scroll: action_scroll,
            alt_click: CustomClickAction::new(action_set, "alt_click", side)?,
            show_hide: CustomClickAction::new(action_set, "show_hide", side)?,
            toggle_dashboard: CustomClickAction::new(action_set, "toggle_dashboard", side)?,
            space_drag: CustomClickAction::new(action_set, "space_drag", side)?,
            space_rotate: CustomClickAction::new(action_set, "space_rotate", side)?,
            space_reset: CustomClickAction::new(action_set, "space_reset", side)?,
            modifier_right: CustomClickAction::new(action_set, "click_modifier_right", side)?,
            modifier_middle: CustomClickAction::new(action_set, "click_modifier_middle", side)?,
            move_mouse: CustomClickAction::new(action_set, "move_mouse", side)?,
            haptics: action_haptics,
        })
    }
}

fn to_path(maybe_path_str: Option<&String>, instance: &xr::Instance) -> Option<xr::Path> {
    maybe_path_str.as_ref().and_then(|s| {
        instance
            .string_to_path(s)
            .inspect_err(|_| {
                log::warn!("Invalid binding path: {s}");
            })
            .ok()
    })
}

fn is_bool(maybe_type_str: Option<&String>) -> bool {
    maybe_type_str
        .as_ref()
        .unwrap() // want panic
        .split('/')
        .next_back()
        .is_some_and(|last| matches!(last, "click" | "touch") || last.starts_with("dpad_"))
}

macro_rules! add_custom {
    ($action:expr, $left:expr, $right:expr, $bindings:expr, $instance:expr) => {
        if let Some(action) = $action.as_ref() {
            if let Some(p) = to_path(action.left.as_ref(), $instance) {
                if is_bool(action.left.as_ref()) {
                    if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.triple.action_bool, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.double.action_bool, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.single.action_bool, p));
                    }
                } else {
                    if let Some(thresholds) = action.threshold {
                        $left.single_threshold.set_thresholds(thresholds);
                        $bindings.push(xr::Binding::new(
                            &$left.single_threshold.handler.action_f32,
                            p,
                        ));
                    } else if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.triple.action_f32, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.double.action_f32, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.single.action_f32, p));
                    }
                }
            }
            if let Some(p) = to_path(action.right.as_ref(), $instance) {
                if is_bool(action.right.as_ref()) {
                    if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.triple.action_bool, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.double.action_bool, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$right.single.action_bool, p));
                    }
                } else {
                    if let Some(thresholds) = action.threshold {
                        $right.single_threshold.set_thresholds(thresholds);
                        $bindings.push(xr::Binding::new(
                            &$right.single_threshold.handler.action_f32,
                            p,
                        ));
                    } else if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.triple.action_f32, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.double.action_f32, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$right.single.action_f32, p));
                    }
                }
            }
        }
    };
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn suggest_bindings(
    instance: &xr::Instance,
    left: &mut OpenXrHandSource,
    right: &mut OpenXrHandSource,
) {
    let profiles = load_action_profiles();

    for profile in profiles {
        let Ok(profile_path) = instance.string_to_path(&profile.profile) else {
            log::debug!("Profile not supported: {}", profile.profile);
            continue;
        };

        let mut bindings: Vec<xr::Binding> = vec![];

        if let Some(action) = profile.pose {
            if let Some(p) = to_path(action.left.as_ref(), instance) {
                bindings.push(xr::Binding::new(&left.pose, p));
            }
            if let Some(p) = to_path(action.right.as_ref(), instance) {
                bindings.push(xr::Binding::new(&right.pose, p));
            }
        }

        if let Some(action) = profile.haptic {
            if let Some(p) = to_path(action.left.as_ref(), instance) {
                bindings.push(xr::Binding::new(&left.haptics, p));
            }
            if let Some(p) = to_path(action.right.as_ref(), instance) {
                bindings.push(xr::Binding::new(&right.haptics, p));
            }
        }

        if let Some(action) = profile.scroll {
            if let Some(p) = to_path(action.left.as_ref(), instance) {
                bindings.push(xr::Binding::new(&left.scroll, p));
            }
            if let Some(p) = to_path(action.right.as_ref(), instance) {
                bindings.push(xr::Binding::new(&right.scroll, p));
            }
        }

        add_custom!(profile.click, left.click, right.click, bindings, instance);

        add_custom!(
            profile.alt_click,
            left.alt_click,
            right.alt_click,
            bindings,
            instance
        );

        add_custom!(profile.grab, left.grab, right.grab, bindings, instance);

        add_custom!(
            profile.show_hide,
            left.show_hide,
            right.show_hide,
            bindings,
            instance
        );

        add_custom!(
            profile.toggle_dashboard,
            left.toggle_dashboard,
            right.toggle_dashboard,
            bindings,
            instance
        );

        add_custom!(
            profile.space_drag,
            left.space_drag,
            right.space_drag,
            bindings,
            instance
        );

        add_custom!(
            profile.space_rotate,
            left.space_rotate,
            right.space_rotate,
            bindings,
            instance
        );

        add_custom!(
            profile.space_reset,
            left.space_reset,
            right.space_reset,
            bindings,
            instance
        );

        add_custom!(
            profile.click_modifier_right,
            left.modifier_right,
            right.modifier_right,
            bindings,
            instance
        );

        add_custom!(
            profile.click_modifier_middle,
            left.modifier_middle,
            right.modifier_middle,
            bindings,
            instance
        );

        add_custom!(
            profile.move_mouse,
            left.move_mouse,
            right.move_mouse,
            bindings,
            instance
        );

        if instance
            .suggest_interaction_profile_bindings(profile_path, &bindings)
            .is_err()
        {
            log::error!("Bad bindings for {}", &profile.profile[22..]);
            log::error!("Verify config: ~/.config/wlxoverlay/openxr_actions.json5");
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenXrActionConfAction {
    left: Option<String>,
    right: Option<String>,
    threshold: Option<[f32; 2]>,
    double_click: Option<bool>,
    triple_click: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenXrActionConfProfile {
    profile: String,
    pose: Option<OpenXrActionConfAction>,
    click: Option<OpenXrActionConfAction>,
    grab: Option<OpenXrActionConfAction>,
    alt_click: Option<OpenXrActionConfAction>,
    show_hide: Option<OpenXrActionConfAction>,
    toggle_dashboard: Option<OpenXrActionConfAction>,
    space_drag: Option<OpenXrActionConfAction>,
    space_rotate: Option<OpenXrActionConfAction>,
    space_reset: Option<OpenXrActionConfAction>,
    click_modifier_right: Option<OpenXrActionConfAction>,
    click_modifier_middle: Option<OpenXrActionConfAction>,
    move_mouse: Option<OpenXrActionConfAction>,
    scroll: Option<OpenXrActionConfAction>,
    haptic: Option<OpenXrActionConfAction>,
}

const DEFAULT_PROFILES: &str = include_str!("openxr_actions.json5");

fn load_action_profiles() -> Vec<OpenXrActionConfProfile> {
    let mut profiles: Vec<OpenXrActionConfProfile> =
        serde_json5::from_str(DEFAULT_PROFILES).unwrap(); // want panic

    let Some(conf) = config_io::load("openxr_actions.json5") else {
        return profiles;
    };

    match serde_json5::from_str::<Vec<OpenXrActionConfProfile>>(&conf) {
        Ok(override_profiles) => {
            for new in override_profiles {
                if let Some(i) = profiles.iter().position(|old| old.profile == new.profile) {
                    profiles[i] = new;
                }
            }
        }
        Err(e) => {
            log::error!("Failed to load openxr_actions.json5: {e}");
        }
    }

    profiles
}
