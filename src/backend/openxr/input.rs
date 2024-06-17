use std::time::{Duration, Instant};

use glam::{bool, Affine3A, Quat, Vec3};
use openxr as xr;
use serde::{Deserialize, Serialize};

use crate::{
    backend::input::{Haptics, Pointer},
    config_io,
    state::{AppSession, AppState},
};

use super::XrState;

type XrSession = xr::Session<xr::Vulkan>;
static DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);

pub(super) struct OpenXrAction {}

pub(super) struct OpenXrInputSource {
    action_set: xr::ActionSet,
    hands: [OpenXrHand; 2],
}

pub(super) struct OpenXrHand {
    source: OpenXrHandSource,
    space: xr::Space,
}

pub struct CustomClickAction {
    action_f32: xr::Action<f32>,
    action_bool: xr::Action<bool>,
    action_f32_double: xr::Action<f32>,
    action_bool_double: xr::Action<bool>,
    last_click: Option<Instant>,
    held: bool,
}

impl CustomClickAction {
    pub fn new(action_set: &xr::ActionSet, name: &str, side: &str) -> anyhow::Result<Self> {
        let action_f32 = action_set.create_action::<f32>(
            &format!("{}_{}_value", side, name),
            &format!("{} hand {} value", side, name),
            &[],
        )?;
        let action_f32_double = action_set.create_action::<f32>(
            &format!("{}_{}_value_double", side, name),
            &format!("{} hand {} value double", side, name),
            &[],
        )?;

        let action_bool = action_set.create_action::<bool>(
            &format!("{}_{}", side, name),
            &format!("{} hand {}", side, name),
            &[],
        )?;

        let action_bool_double = action_set.create_action::<bool>(
            &format!("{}_{}_double", side, name),
            &format!("{} hand {} double", side, name),
            &[],
        )?;

        Ok(Self {
            action_f32,
            action_f32_double,
            action_bool,
            action_bool_double,
            last_click: None,
            held: false,
        })
    }
    pub fn state(
        &mut self,
        before: bool,
        state: &XrState,
        session: &AppSession,
    ) -> anyhow::Result<bool> {
        let res = self.action_bool.state(&state.session, xr::Path::NULL)?;
        if res.is_active && res.current_state {
            return Ok(true);
        }

        let res = self
            .action_bool_double
            .state(&state.session, xr::Path::NULL)?;
        if res.is_active
            && ((before && res.current_state) || self.check_double_click(res.current_state))
        {
            return Ok(true);
        }

        let threshold = if before {
            session.config.xr_click_sensitivity_release
        } else {
            session.config.xr_click_sensitivity
        };

        let res = self.action_f32.state(&state.session, xr::Path::NULL)?;
        if res.is_active && res.current_state > threshold {
            return Ok(true);
        }

        let res = self.action_f32.state(&state.session, xr::Path::NULL)?;
        if res.is_active
            && ((before && res.current_state > threshold)
                || self.check_double_click(res.current_state > threshold))
        {
            return Ok(true);
        }

        Ok(false)
    }

    // submit a click. returns true if it should count as a double click
    fn check_double_click(&mut self, state: bool) -> bool {
        if !state {
            self.held = false;
            return false;
        }

        if self.held {
            return false;
        }

        let now = Instant::now();
        let double_click = match self.last_click {
            Some(last_click) => now - last_click < DOUBLE_CLICK_TIME,
            None => false,
        };
        self.last_click = if double_click { None } else { Some(now) };
        self.held = true;
        double_click
    }
}

pub(super) struct OpenXrHandSource {
    action_pose: xr::Action<xr::Posef>,
    action_click: CustomClickAction,
    action_grab: CustomClickAction,
    action_alt_click: CustomClickAction,
    action_show_hide: CustomClickAction,
    action_space_drag: CustomClickAction,
    action_space_rotate: CustomClickAction,
    action_modifier_right: CustomClickAction,
    action_modifier_middle: CustomClickAction,
    action_move_mouse: CustomClickAction,
    action_scroll: xr::Action<f32>,
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

        for i in 0..2 {
            self.hands[i].update(&mut state.input_state.pointers[i], xr, &state.session)?;
        }
        Ok(())
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
            let quat = unsafe { std::mem::transmute::<_, Quat>(location.pose.orientation) };
            let pos = unsafe { std::mem::transmute::<_, Vec3>(location.pose.position) };
            pointer.pose = Affine3A::from_rotation_translation(quat, pos);
        }

        pointer.now.click = self
            .source
            .action_click
            .state(pointer.before.click, xr, session)?;

        pointer.now.grab = self
            .source
            .action_grab
            .state(pointer.before.grab, xr, session)?;

        pointer.now.scroll = self
            .source
            .action_scroll
            .state(&xr.session, xr::Path::NULL)?
            .current_state;

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

        let action_scroll = action_set.create_action::<f32>(
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
            action_space_drag: CustomClickAction::new(action_set, "space_drag", side)?,
            action_space_rotate: CustomClickAction::new(action_set, "space_rotate", side)?,
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
        .unwrap()
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
                    if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.action_bool_double, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.action_bool, p));
                    }
                } else {
                    if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$left.action_f32_double, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$left.action_f32, p));
                    }
                }
            }
            if let Some(p) = to_path(&action.right, $instance) {
                if is_bool(&action.right) {
                    if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.action_bool_double, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$right.action_bool, p));
                    }
                } else {
                    if action.double_click.unwrap_or(false) {
                        $bindings.push(xr::Binding::new(&$right.action_f32_double, p));
                    } else {
                        $bindings.push(xr::Binding::new(&$right.action_f32, p));
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenXrActionConfProfile {
    profile: String,
    pose: Option<OpenXrActionConfAction>,
    click: Option<OpenXrActionConfAction>,
    grab: Option<OpenXrActionConfAction>,
    alt_click: Option<OpenXrActionConfAction>,
    show_hide: Option<OpenXrActionConfAction>,
    space_drag: Option<OpenXrActionConfAction>,
    space_rotate: Option<OpenXrActionConfAction>,
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
