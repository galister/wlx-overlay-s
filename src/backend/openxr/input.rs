use std::time::{Duration, Instant};

use glam::{bool, Affine3A, Quat, Vec3};
use openxr as xr;

use crate::{backend::input::Pointer, state::AppState};

use super::XrState;

type XrSession = xr::Session<xr::Vulkan>;
static DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);

pub(super) struct DoubleClickCounter {
    pub(super) last_click: Option<Instant>,
}

impl DoubleClickCounter {
    pub(super) fn new() -> Self {
        Self { last_click: None }
    }

    // submit a click. returns true if it should count as a double click
    pub(super) fn click(&mut self) -> bool {
        let now = Instant::now();
        let double_click = match self.last_click {
            Some(last_click) => now - last_click < DOUBLE_CLICK_TIME,
            None => false,
        };
        self.last_click = if double_click { None } else { Some(now) };
        double_click
    }
}

pub(super) struct OpenXrInputSource {
    action_set: xr::ActionSet,
    hands: [OpenXrHand; 2],
}

pub(super) struct OpenXrHand {
    source: OpenXrHandSource,
    space: xr::Space,
}

pub(super) struct OpenXrHandSource {
    action_pose: xr::Action<xr::Posef>,
    action_click: xr::Action<f32>,
    action_grab: xr::Action<f32>,
    action_scroll: xr::Action<f32>,
    action_alt_click: xr::Action<f32>,
    action_show_hide: xr::Action<bool>,
    action_click_modifier_right: xr::Action<bool>,
    action_click_modifier_middle: xr::Action<bool>,
    action_haptics: xr::Action<xr::Haptic>,
}

impl OpenXrInputSource {
    pub fn new(xr: &XrState) -> Self {
        let mut action_set = xr
            .session
            .instance()
            .create_action_set("wlx-overlay-s", "WlxOverlay-S Actions", 0)
            .expect("Failed to create action set");

        let left_source = OpenXrHandSource::new(&mut action_set, "left");
        let right_source = OpenXrHandSource::new(&mut action_set, "right");

        suggest_bindings(&xr.instance, &[&left_source, &right_source]);

        xr.session.attach_action_sets(&[&action_set]).unwrap();

        Self {
            action_set,
            hands: [
                OpenXrHand::new(&xr, left_source),
                OpenXrHand::new(&xr, right_source),
            ],
        }
    }

    pub fn update(&self, xr: &XrState, state: &mut AppState) {
        xr.session
            .sync_actions(&[(&self.action_set).into()])
            .unwrap();

        for i in 0..2 {
            self.hands[i].update(
                &mut state.input_state.pointers[i],
                &xr.stage,
                &xr.session,
                xr.predicted_display_time,
            );
        }
    }
}

impl OpenXrHand {
    pub(super) fn new(xr: &XrState, source: OpenXrHandSource) -> Self {
        let space = source
            .action_pose
            .create_space(xr.session.clone(), xr::Path::NULL, xr::Posef::IDENTITY)
            .unwrap();

        Self { source, space }
    }

    pub(super) fn update(
        &self,
        pointer: &mut Pointer,
        stage: &xr::Space,
        session: &XrSession,
        time: xr::Time,
    ) {
        let location = self.space.locate(stage, time).unwrap();
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
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state
            > 0.7;

        pointer.now.grab = self
            .source
            .action_grab
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state
            > 0.7;

        pointer.now.scroll = self
            .source
            .action_scroll
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;

        pointer.now.alt_click = self
            .source
            .action_alt_click
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state
            > 0.7;

        pointer.now.show_hide = self
            .source
            .action_show_hide
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;

        pointer.now.click_modifier_right = self
            .source
            .action_click_modifier_right
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;

        pointer.now.click_modifier_middle = self
            .source
            .action_click_modifier_middle
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;
    }
}

// supported action types: Haptic, Posef, Vector2f, f32, bool
impl OpenXrHandSource {
    pub(super) fn new(action_set: &mut xr::ActionSet, side: &str) -> Self {
        let action_pose = action_set
            .create_action::<xr::Posef>(
                &format!("{}_hand", side),
                &format!("{} hand pose", side),
                &[],
            )
            .unwrap();

        let action_click = action_set
            .create_action::<f32>(
                &format!("{}_click", side),
                &format!("{} hand click", side),
                &[],
            )
            .unwrap();
        let action_grab = action_set
            .create_action::<f32>(
                &format!("{}_grab", side),
                &format!("{} hand grab", side),
                &[],
            )
            .unwrap();
        let action_scroll = action_set
            .create_action::<f32>(
                &format!("{}_scroll", side),
                &format!("{} hand scroll", side),
                &[],
            )
            .unwrap();
        let action_alt_click = action_set
            .create_action::<f32>(
                &format!("{}_alt_click", side),
                &format!("{} hand alt click", side),
                &[],
            )
            .unwrap();
        let action_show_hide = action_set
            .create_action::<bool>(
                &format!("{}_show_hide", side),
                &format!("{} hand show/hide", side),
                &[],
            )
            .unwrap();
        let action_click_modifier_right = action_set
            .create_action::<bool>(
                &format!("{}_click_modifier_right", side),
                &format!("{} hand right click modifier", side),
                &[],
            )
            .unwrap();
        let action_click_modifier_middle = action_set
            .create_action::<bool>(
                &format!("{}_click_modifier_middle", side),
                &format!("{} hand middle click modifier", side),
                &[],
            )
            .unwrap();
        let action_haptics = action_set
            .create_action::<xr::Haptic>(
                &format!("{}_haptics", side),
                &format!("{} hand haptics", side),
                &[],
            )
            .unwrap();

        Self {
            action_pose,
            action_click,
            action_grab,
            action_scroll,
            action_alt_click,
            action_show_hide,
            action_click_modifier_right,
            action_click_modifier_middle,
            action_haptics,
        }
    }
}

fn suggest_bindings(instance: &xr::Instance, hands: &[&OpenXrHandSource; 2]) {
    let path = instance
        .string_to_path("/interaction_profiles/khr/simple_controller")
        .unwrap();

    // not fully functional, but helpful for debugging
    instance
        .suggest_interaction_profile_bindings(
            path,
            &[
                xr::Binding::new(
                    &hands[0].action_pose,
                    instance
                        .string_to_path("/user/hand/left/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_pose,
                    instance
                        .string_to_path("/user/hand/right/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click,
                    instance
                        .string_to_path("/user/hand/left/input/select/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click,
                    instance
                        .string_to_path("/user/hand/right/input/select/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_show_hide,
                    instance
                        .string_to_path("/user/hand/left/input/menu/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_haptics,
                    instance
                        .string_to_path("/user/hand/left/output/haptic")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_haptics,
                    instance
                        .string_to_path("/user/hand/right/output/haptic")
                        .unwrap(),
                ),
            ],
        )
        .unwrap();

    let path = instance
        .string_to_path("/interaction_profiles/oculus/touch_controller")
        .unwrap();
    instance
        .suggest_interaction_profile_bindings(
            path,
            &[
                xr::Binding::new(
                    &hands[0].action_pose,
                    instance
                        .string_to_path("/user/hand/left/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_pose,
                    instance
                        .string_to_path("/user/hand/right/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click,
                    instance
                        .string_to_path("/user/hand/left/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click,
                    instance
                        .string_to_path("/user/hand/right/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_grab,
                    instance
                        .string_to_path("/user/hand/left/input/squeeze/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_grab,
                    instance
                        .string_to_path("/user/hand/right/input/squeeze/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_scroll,
                    instance
                        .string_to_path("/user/hand/left/input/thumbstick/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_scroll,
                    instance
                        .string_to_path("/user/hand/right/input/thumbstick/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_show_hide,
                    instance
                        .string_to_path("/user/hand/left/input/y/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click_modifier_right,
                    instance
                        .string_to_path("/user/hand/left/input/y/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click_modifier_right,
                    instance
                        .string_to_path("/user/hand/right/input/b/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click_modifier_middle,
                    instance
                        .string_to_path("/user/hand/left/input/x/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click_modifier_middle,
                    instance
                        .string_to_path("/user/hand/right/input/a/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_haptics,
                    instance
                        .string_to_path("/user/hand/left/output/haptic")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_haptics,
                    instance
                        .string_to_path("/user/hand/right/output/haptic")
                        .unwrap(),
                ),
            ],
        )
        .unwrap();

    let path = instance
        .string_to_path("/interaction_profiles/valve/index_controller")
        .unwrap();
    instance
        .suggest_interaction_profile_bindings(
            path,
            &[
                xr::Binding::new(
                    &hands[0].action_pose,
                    instance
                        .string_to_path("/user/hand/left/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_pose,
                    instance
                        .string_to_path("/user/hand/right/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click,
                    instance
                        .string_to_path("/user/hand/left/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click,
                    instance
                        .string_to_path("/user/hand/right/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_grab,
                    instance
                        .string_to_path("/user/hand/left/input/squeeze/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_grab,
                    instance
                        .string_to_path("/user/hand/right/input/squeeze/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_scroll,
                    instance
                        .string_to_path("/user/hand/left/input/thumbstick/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_scroll,
                    instance
                        .string_to_path("/user/hand/right/input/thumbstick/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_alt_click,
                    instance
                        .string_to_path("/user/hand/left/input/trackpad/force")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_alt_click,
                    instance
                        .string_to_path("/user/hand/right/input/trackpad/force")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_show_hide,
                    instance
                        .string_to_path("/user/hand/left/input/b/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click_modifier_right,
                    instance
                        .string_to_path("/user/hand/left/input/b/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click_modifier_right,
                    instance
                        .string_to_path("/user/hand/right/input/b/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click_modifier_middle,
                    instance
                        .string_to_path("/user/hand/left/input/a/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click_modifier_middle,
                    instance
                        .string_to_path("/user/hand/right/input/a/touch")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_haptics,
                    instance
                        .string_to_path("/user/hand/left/output/haptic")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_haptics,
                    instance
                        .string_to_path("/user/hand/right/output/haptic")
                        .unwrap(),
                ),
            ],
        )
        .unwrap();

    let path = instance
        .string_to_path("/interaction_profiles/htc/vive_controller")
        .unwrap();
    instance
        .suggest_interaction_profile_bindings(
            path,
            &[
                xr::Binding::new(
                    &hands[0].action_pose,
                    instance
                        .string_to_path("/user/hand/left/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_pose,
                    instance
                        .string_to_path("/user/hand/right/input/aim/pose")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_click,
                    instance
                        .string_to_path("/user/hand/left/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_click,
                    instance
                        .string_to_path("/user/hand/right/input/trigger/value")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_grab,
                    instance
                        .string_to_path("/user/hand/left/input/squeeze/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_grab,
                    instance
                        .string_to_path("/user/hand/right/input/squeeze/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_scroll,
                    instance
                        .string_to_path("/user/hand/left/input/trackpad/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_scroll,
                    instance
                        .string_to_path("/user/hand/right/input/trackpad/y")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_show_hide,
                    instance
                        .string_to_path("/user/hand/left/input/menu/click")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[0].action_haptics,
                    instance
                        .string_to_path("/user/hand/left/output/haptic")
                        .unwrap(),
                ),
                xr::Binding::new(
                    &hands[1].action_haptics,
                    instance
                        .string_to_path("/user/hand/right/output/haptic")
                        .unwrap(),
                ),
            ],
        )
        .unwrap();
}
