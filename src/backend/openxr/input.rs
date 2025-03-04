use std::{
    array::from_fn,
    mem::transmute,
    time::{Duration, Instant},
};

use glam::{bool, Affine3A, Quat, Vec3};
use libmonado as mnd;
use openxr::{self as xr, Quaternionf, Vector2f, Vector3f};
use serde::{Deserialize, Serialize};

use crate::{
    backend::input::{Haptics, Pointer, TrackedDevice, TrackedDeviceRole},
    config_io,
    state::{AppSession, AppState},
};

use super::{helpers::posef_to_transform, XrState};

type XrSession = xr::Session<xr::Vulkan>;

static CLICK_TIMES: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_millis(500),
    Duration::from_millis(750),
];

pub(super) struct OpenXrAction {}

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

impl<const COUNT: usize> MultiClickHandler<COUNT> {
    fn new(action_set: &xr::ActionSet, action_name: &str, side: &str) -> anyhow::Result<Self> {
        let name = format!("{}_{}-{}", side, COUNT, action_name);
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
            let long_ago = Instant::now() - Duration::from_secs(10);
            self.previous
                .iter_mut()
                .for_each(|instant| *instant = long_ago)
        } else if COUNT > 0 {
            log::trace!("{}: rotate", self.name);
            self.previous.rotate_right(1);
            self.previous[0] = Instant::now();
            self.held_inactive = true;
        }

        Ok(passed)
    }
}

pub struct CustomClickAction {
    single: MultiClickHandler<0>,
    double: MultiClickHandler<1>,
    triple: MultiClickHandler<2>,
}

impl CustomClickAction {
    pub fn new(action_set: &xr::ActionSet, name: &str, side: &str) -> anyhow::Result<Self> {
        let single = MultiClickHandler::new(action_set, name, side)?;
        let double = MultiClickHandler::new(action_set, name, side)?;
        let triple = MultiClickHandler::new(action_set, name, side)?;

        Ok(Self {
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
            || self.triple.check(&state.session, threshold)?)
    }
}

pub(super) struct OpenXrHandSource {
    action_pose: xr::Action<xr::Posef>,
    action_click: CustomClickAction,
    action_grab: CustomClickAction,
    action_alt_click: CustomClickAction,
    action_show_hide: CustomClickAction,
    action_toggle_dashboard: CustomClickAction,
    action_space_drag: CustomClickAction,
    action_space_rotate: CustomClickAction,
    action_space_reset: CustomClickAction,
    action_modifier_right: CustomClickAction,
    action_modifier_middle: CustomClickAction,
    action_move_mouse: CustomClickAction,
    action_scroll: xr::Action<Vector2f>,
    action_haptics: xr::Action<xr::Haptic>,
}

impl OpenXrInputSource {
    pub fn new(xr: &XrState) -> anyhow::Result<Self> {
        let mut action_set =
            xr.session
                .instance()
                .create_action_set("wlx-overlay-s", "WlxOverlay-S Actions", 0)?;

        let left_source = OpenXrHandSource::new(&mut action_set, "left")?;
        let right_source = OpenXrHandSource::new(&mut action_set, "right")?;

        suggest_bindings(&xr.instance, &[&left_source, &right_source])?;

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
        let action = &self.hands[hand].source.action_haptics;

        let duration_nanos = (haptics.duration as f64) * 1_000_000_000.0;

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
        if let Ok(status) = device.battery_status() {
            if status.present {
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
    }

    pub fn update_devices(&mut self, app: &mut AppState, monado: &mut mnd::Monado) {
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
            if let Ok(mut device) = device {
                if !seen.contains(&device.index) {
                    seen.push(device.index);
                    Self::update_device_battery_status(&mut device, wlx_role, app);
                }
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
            (a.soc.is_none() as u8)
                .cmp(&(b.soc.is_none() as u8))
                .then((a.role as u8).cmp(&(b.role as u8)))
                .then(a.soc.unwrap_or(999.).total_cmp(&b.soc.unwrap_or(999.)))
        });
    }
}

impl OpenXrHand {
    pub(super) fn new(xr: &XrState, source: OpenXrHandSource) -> Result<Self, xr::sys::Result> {
        let space = source.action_pose.create_space(
            xr.session.clone(),
            xr::Path::NULL,
            xr::Posef::IDENTITY,
        )?;

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

        pointer.now.click = self
            .source
            .action_click
            .state(pointer.before.click, xr, session)?;

        pointer.now.grab = self
            .source
            .action_grab
            .state(pointer.before.grab, xr, session)?;

        let scroll = self
            .source
            .action_scroll
            .state(&xr.session, xr::Path::NULL)?
            .current_state;

        pointer.now.scroll_x = scroll.x;
        pointer.now.scroll_y = scroll.x;

        pointer.now.alt_click =
            self.source
                .action_alt_click
                .state(pointer.before.alt_click, xr, session)?;

        pointer.now.show_hide =
            self.source
                .action_show_hide
                .state(pointer.before.show_hide, xr, session)?;

        pointer.now.click_modifier_right = self.source.action_modifier_right.state(
            pointer.before.click_modifier_right,
            xr,
            session,
        )?;

        pointer.now.toggle_dashboard = self.source.action_toggle_dashboard.state(
            pointer.before.toggle_dashboard,
            xr,
            session,
        )?;

        pointer.now.click_modifier_middle = self.source.action_modifier_middle.state(
            pointer.before.click_modifier_middle,
            xr,
            session,
        )?;

        pointer.now.move_mouse =
            self.source
                .action_move_mouse
                .state(pointer.before.move_mouse, xr, session)?;

        pointer.now.space_drag =
            self.source
                .action_space_drag
                .state(pointer.before.space_drag, xr, session)?;

        pointer.now.space_rotate =
            self.source
                .action_space_rotate
                .state(pointer.before.space_rotate, xr, session)?;

        pointer.now.space_reset =
            self.source
                .action_space_reset
                .state(pointer.before.space_reset, xr, session)?;

        Ok(())
    }
}

// supported action types: Haptic, Posef, Vector2f, f32, bool
impl OpenXrHandSource {
    pub(super) fn new(action_set: &mut xr::ActionSet, side: &str) -> anyhow::Result<Self> {
        let action_pose = action_set.create_action::<xr::Posef>(
            &format!("{}_hand", side),
            &format!("{} hand pose", side),
            &[],
        )?;

        let action_scroll = action_set.create_action::<Vector2f>(
            &format!("{}_scroll", side),
            &format!("{} hand scroll", side),
            &[],
        )?;
        let action_haptics = action_set.create_action::<xr::Haptic>(
            &format!("{}_haptics", side),
            &format!("{} hand haptics", side),
            &[],
        )?;

        Ok(Self {
            action_pose,
            action_click: CustomClickAction::new(action_set, "click", side)?,
            action_grab: CustomClickAction::new(action_set, "grab", side)?,
            action_scroll,
            action_alt_click: CustomClickAction::new(action_set, "alt_click", side)?,
            action_show_hide: CustomClickAction::new(action_set, "show_hide", side)?,
            action_toggle_dashboard: CustomClickAction::new(action_set, "toggle_dashboard", side)?,
            action_space_drag: CustomClickAction::new(action_set, "space_drag", side)?,
            action_space_rotate: CustomClickAction::new(action_set, "space_rotate", side)?,
            action_space_reset: CustomClickAction::new(action_set, "space_reset", side)?,
            action_modifier_right: CustomClickAction::new(
                action_set,
                "click_modifier_right",
                side,
            )?,
            action_modifier_middle: CustomClickAction::new(
                action_set,
                "click_modifier_middle",
                side,
            )?,
            action_move_mouse: CustomClickAction::new(action_set, "move_mouse", side)?,
            action_haptics,
        })
    }
}

fn to_path(maybe_path_str: &Option<String>, instance: &xr::Instance) -> Option<xr::Path> {
    maybe_path_str
        .as_ref()
        .and_then(|s| match instance.string_to_path(s) {
            Ok(path) => Some(path),
            Err(_) => {
                log::warn!("Invalid binding path: {}", s);
                None
            }
        })
}

fn is_bool(maybe_type_str: &Option<String>) -> bool {
    maybe_type_str
        .as_ref()
        .unwrap() // want panic
        .split('/')
        .last()
        .map(|last| matches!(last, "click" | "touch"))
        .unwrap_or(false)
}

macro_rules! add_custom {
    ($action:expr, $left:expr, $right:expr, $bindings:expr, $instance:expr) => {
        if let Some(action) = $action.as_ref() {
            if let Some(p) = to_path(&action.left, $instance) {
                if is_bool(&action.left) {
                    if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.triple.action_bool, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.double.action_bool, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.single.action_bool, p));
                    }
                } else {
                    if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.triple.action_f32, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.double.action_f32, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.single.action_f32, p));
                    }
                }
            }
            if let Some(p) = to_path(&action.right, $instance) {
                if is_bool(&action.right) {
                    if action.triple_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.triple.action_bool, p));
                    } else if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.double.action_bool, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$right.single.action_bool, p));
                    }
                } else {
                    if action.triple_click.unwrap_or(false) {
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

fn suggest_bindings(instance: &xr::Instance, hands: &[&OpenXrHandSource; 2]) -> anyhow::Result<()> {
    let profiles = load_action_profiles()?;

    for profile in profiles {
        let Ok(profile_path) = instance.string_to_path(&profile.profile) else {
            log::debug!("Profile not supported: {}", profile.profile);
            continue;
        };

        let mut bindings: Vec<xr::Binding> = vec![];

        if let Some(action) = profile.pose {
            if let Some(p) = to_path(&action.left, instance) {
                bindings.push(xr::Binding::new(&hands[0].action_pose, p));
            }
            if let Some(p) = to_path(&action.right, instance) {
                bindings.push(xr::Binding::new(&hands[1].action_pose, p));
            }
        }

        if let Some(action) = profile.haptic {
            if let Some(p) = to_path(&action.left, instance) {
                bindings.push(xr::Binding::new(&hands[0].action_haptics, p));
            }
            if let Some(p) = to_path(&action.right, instance) {
                bindings.push(xr::Binding::new(&hands[1].action_haptics, p));
            }
        }

        if let Some(action) = profile.scroll {
            if let Some(p) = to_path(&action.left, instance) {
                bindings.push(xr::Binding::new(&hands[0].action_scroll, p));
            }
            if let Some(p) = to_path(&action.right, instance) {
                bindings.push(xr::Binding::new(&hands[1].action_scroll, p));
            }
        }

        add_custom!(
            profile.click,
            hands[0].action_click,
            hands[1].action_click,
            bindings,
            instance
        );

        add_custom!(
            profile.alt_click,
            &hands[0].action_alt_click,
            &hands[1].action_alt_click,
            bindings,
            instance
        );

        add_custom!(
            profile.grab,
            &hands[0].action_grab,
            &hands[1].action_grab,
            bindings,
            instance
        );

        add_custom!(
            profile.show_hide,
            &hands[0].action_show_hide,
            &hands[1].action_show_hide,
            bindings,
            instance
        );

        add_custom!(
            profile.toggle_dashboard,
            &hands[0].action_toggle_dashboard,
            &hands[1].action_toggle_dashboard,
            bindings,
            instance
        );

        add_custom!(
            profile.space_drag,
            &hands[0].action_space_drag,
            &hands[1].action_space_drag,
            bindings,
            instance
        );

        add_custom!(
            profile.space_rotate,
            &hands[0].action_space_rotate,
            &hands[1].action_space_rotate,
            bindings,
            instance
        );

        add_custom!(
            profile.space_reset,
            &hands[0].action_space_reset,
            &hands[1].action_space_reset,
            bindings,
            instance
        );

        add_custom!(
            profile.click_modifier_right,
            &hands[0].action_modifier_right,
            &hands[1].action_modifier_right,
            bindings,
            instance
        );

        add_custom!(
            profile.click_modifier_middle,
            &hands[0].action_modifier_middle,
            &hands[1].action_modifier_middle,
            bindings,
            instance
        );

        add_custom!(
            profile.move_mouse,
            &hands[0].action_move_mouse,
            &hands[1].action_move_mouse,
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

    Ok(())
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

fn load_action_profiles() -> anyhow::Result<Vec<OpenXrActionConfProfile>> {
    let mut profiles: Vec<OpenXrActionConfProfile> =
        serde_json5::from_str(DEFAULT_PROFILES).unwrap(); // want panic

    let Some(conf) = config_io::load("openxr_actions.json5") else {
        return Ok(profiles);
    };

    match serde_json5::from_str::<Vec<OpenXrActionConfProfile>>(&conf) {
        Ok(override_profiles) => {
            override_profiles.into_iter().for_each(|new| {
                if let Some(i) = profiles.iter().position(|old| old.profile == new.profile) {
                    profiles[i] = new;
                }
            });
        }
        Err(e) => {
            log::error!("Failed to load openxr_actions.json5: {}", e);
        }
    }

    Ok(profiles)
}
