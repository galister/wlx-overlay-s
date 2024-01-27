use glam::{bool, Affine3A, Quat, Vec3};
use openxr as xr;

use crate::{backend::input::Pointer, state::AppState};

type XrSession = xr::Session<xr::Vulkan>;

pub(super) struct OpenXrInputSource {
    action_set: xr::ActionSet,
    hands: [OpenXrHand; 2],
    pub(super) stage: xr::Space,
}

pub(super) struct OpenXrHand {
    source: OpenXrHandSource,
    space: xr::Space,
}

pub(super) struct OpenXrHandSource {
    action_pose: xr::Action<xr::Posef>,
    action_click: xr::Action<bool>,
    action_grab: xr::Action<bool>,
    action_scroll: xr::Action<xr::Vector2f>,
    action_alt_click: xr::Action<bool>,
    action_show_hide: xr::Action<bool>,
    action_click_modifier_right: xr::Action<bool>,
    action_click_modifier_middle: xr::Action<bool>,
    action_haptics: xr::Action<xr::Haptic>,
}

impl OpenXrInputSource {
    pub fn new(session: XrSession) -> Self {
        let mut action_set = session
            .instance()
            .create_action_set("wlx-overlay-s", "WlxOverlay-S Actions", 0)
            .expect("Failed to create action set");

        let left_source = OpenXrHandSource::new(&mut action_set, "left");
        let right_source = OpenXrHandSource::new(&mut action_set, "right");

        session.attach_action_sets(&[&action_set]).unwrap();

        let stage = session
            .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)
            .unwrap();

        Self {
            action_set,
            hands: [
                OpenXrHand::new(session.clone(), left_source),
                OpenXrHand::new(session, right_source),
            ],
            stage,
        }
    }

    pub fn update(&self, session: &XrSession, time: xr::Time, state: &mut AppState) {
        session.sync_actions(&[(&self.action_set).into()]).unwrap();

        for i in 0..2 {
            self.hands[i].update(
                &mut state.input_state.pointers[i],
                &self.stage,
                session,
                time,
            );
        }
    }
}

impl OpenXrHand {
    pub(super) fn new(session: XrSession, source: OpenXrHandSource) -> Self {
        let space = source
            .action_pose
            .create_space(session.clone(), xr::Path::NULL, xr::Posef::IDENTITY)
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
            .current_state;

        pointer.now.grab = self
            .source
            .action_grab
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;

        pointer.now.scroll = self
            .source
            .action_scroll
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state
            .y;

        pointer.now.alt_click = self
            .source
            .action_alt_click
            .state(session, xr::Path::NULL)
            .unwrap()
            .current_state;

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
            .create_action::<bool>(
                &format!("{}_click", side),
                &format!("{} hand click", side),
                &[],
            )
            .unwrap();
        let action_grab = action_set
            .create_action::<bool>(
                &format!("{}_grab", side),
                &format!("{} hand grab", side),
                &[],
            )
            .unwrap();
        let action_scroll = action_set
            .create_action::<xr::Vector2f>(
                &format!("{}_scroll", side),
                &format!("{} hand scroll", side),
                &[],
            )
            .unwrap();
        let action_alt_click = action_set
            .create_action::<bool>(
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

        // TODO suggest bindings

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
