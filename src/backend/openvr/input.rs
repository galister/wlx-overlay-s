use std::{array, fs::File, io::Write, time::Duration};

use anyhow::bail;
use ovr_overlay::{
    input::{ActionHandle, ActionSetHandle, ActiveActionSet, InputManager, InputValueHandle},
    sys::{
        k_unMaxTrackedDeviceCount, ETrackedControllerRole, ETrackedDeviceClass,
        ETrackedDeviceProperty, ETrackingUniverseOrigin,
    },
    system::SystemManager,
    TrackedDeviceIndex,
};

use crate::{
    backend::input::{Haptics, TrackedDevice, TrackedDeviceRole},
    config_io::CONFIG_ROOT_PATH,
    state::AppState,
};

use super::helpers::Affine3AConvert;

macro_rules! result_str {
    ( $e:expr ) => {
        match $e {
            Ok(x) => Ok(x),
            Err(y) => Err(y.description()),
        }
    };
}

const SET_DEFAULT: &str = "/actions/default";
const INPUT_SOURCES: [&str; 2] = ["/user/hand/left", "/user/hand/right"];
const PATH_POSES: [&str; 2] = [
    "/actions/default/in/LeftHand",
    "/actions/default/in/RightHand",
];
const PATH_HAPTICS: [&str; 2] = [
    "/actions/default/out/HapticsLeft",
    "/actions/default/out/HapticsRight",
];

const PATH_CLICK: &str = "/actions/default/in/Click";
const PATH_GRAB: &str = "/actions/default/in/Grab";
const PATH_SCROLL: &str = "/actions/default/in/Scroll";
const PATH_ALT_CLICK: &str = "/actions/default/in/AltClick";
const PATH_SHOW_HIDE: &str = "/actions/default/in/ShowHide";
const PATH_SPACE_DRAG: &str = "/actions/default/in/SpaceDrag";
const PATH_CLICK_MODIFIER_RIGHT: &str = "/actions/default/in/ClickModifierRight";
const PATH_CLICK_MODIFIER_MIDDLE: &str = "/actions/default/in/ClickModifierMiddle";

const INPUT_ANY: InputValueHandle = InputValueHandle(ovr_overlay::sys::k_ulInvalidInputValueHandle);

pub(super) struct OpenVrInputSource {
    hands: [OpenVrHandSource; 2],
    set_hnd: ActionSetHandle,
    click_hnd: ActionHandle,
    grab_hnd: ActionHandle,
    scroll_hnd: ActionHandle,
    alt_click_hnd: ActionHandle,
    show_hide_hnd: ActionHandle,
    space_drag_hnd: ActionHandle,
    click_modifier_right_hnd: ActionHandle,
    click_modifier_middle_hnd: ActionHandle,
}

pub(super) struct OpenVrHandSource {
    has_pose: bool,
    input_hnd: InputValueHandle,
    pose_hnd: ActionHandle,
    haptics_hnd: ActionHandle,
}

impl OpenVrInputSource {
    pub fn new(input: &mut InputManager) -> Result<Self, &'static str> {
        let set_hnd = result_str!(input.get_action_set_handle(SET_DEFAULT))?;

        let click_hnd = result_str!(input.get_action_handle(PATH_CLICK))?;
        let grab_hnd = result_str!(input.get_action_handle(PATH_GRAB))?;
        let scroll_hnd = result_str!(input.get_action_handle(PATH_SCROLL))?;
        let alt_click_hnd = result_str!(input.get_action_handle(PATH_ALT_CLICK))?;
        let show_hide_hnd = result_str!(input.get_action_handle(PATH_SHOW_HIDE))?;
        let space_drag_hnd = result_str!(input.get_action_handle(PATH_SPACE_DRAG))?;
        let click_modifier_right_hnd =
            result_str!(input.get_action_handle(PATH_CLICK_MODIFIER_RIGHT))?;
        let click_modifier_middle_hnd =
            result_str!(input.get_action_handle(PATH_CLICK_MODIFIER_MIDDLE))?;

        let input_hnd: Vec<InputValueHandle> = INPUT_SOURCES
            .iter()
            .map(|path| Ok(result_str!(input.get_input_source_handle(path))?))
            .collect::<Result<_, &'static str>>()?;

        let pose_hnd: Vec<ActionHandle> = PATH_POSES
            .iter()
            .map(|path| Ok(result_str!(input.get_action_handle(path))?))
            .collect::<Result<_, &'static str>>()?;

        let haptics_hnd: Vec<ActionHandle> = PATH_HAPTICS
            .iter()
            .map(|path| Ok(result_str!(input.get_action_handle(path))?))
            .collect::<Result<_, &'static str>>()?;

        let hands: [OpenVrHandSource; 2] = array::from_fn(|i| OpenVrHandSource {
            has_pose: false,
            input_hnd: input_hnd[i],
            pose_hnd: pose_hnd[i],
            haptics_hnd: haptics_hnd[i],
        });

        Ok(OpenVrInputSource {
            set_hnd,
            click_hnd,
            grab_hnd,
            scroll_hnd,
            alt_click_hnd,
            show_hide_hnd,
            space_drag_hnd,
            click_modifier_right_hnd,
            click_modifier_middle_hnd,
            hands,
        })
    }

    pub fn haptics(&mut self, input: &mut InputManager, hand: usize, haptics: &Haptics) {
        let hnd = self.hands[hand].haptics_hnd;
        let _ = input.trigger_haptic_vibration_action(
            hnd,
            0.0,
            Duration::from_secs_f32(haptics.duration),
            haptics.frequency,
            haptics.intensity,
            INPUT_ANY,
        );
    }

    pub fn update(
        &mut self,
        input: &mut InputManager,
        system: &mut SystemManager,
        app: &mut AppState,
    ) {
        let aas = ActiveActionSet {
            0: ovr_overlay::sys::VRActiveActionSet_t {
                ulActionSet: self.set_hnd.0,
                ulRestrictedToDevice: 0,
                ulSecondaryActionSet: 0,
                unPadding: 0,
                nPriority: 0,
            },
        };

        let _ = input.update_actions(&mut [aas]);

        let universe = ETrackingUniverseOrigin::TrackingUniverseStanding;

        for i in 0..2 {
            let hand = &mut self.hands[i];
            let app_hand = &mut app.input_state.pointers[i];

            hand.has_pose = false;

            let _ = input
                .get_pose_action_data_relative_to_now(
                    hand.pose_hnd,
                    universe.clone(),
                    0.005,
                    INPUT_ANY,
                )
                .and_then(|pose| {
                    app_hand.pose = pose.0.pose.mDeviceToAbsoluteTracking.to_affine();
                    hand.has_pose = true;
                    Ok(())
                });

            app_hand.now.click = input
                .get_digital_action_data(self.click_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.grab = input
                .get_digital_action_data(self.grab_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.alt_click = input
                .get_digital_action_data(self.alt_click_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.show_hide = input
                .get_digital_action_data(self.show_hide_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.space_drag = input
                .get_digital_action_data(self.space_drag_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.click_modifier_right = input
                .get_digital_action_data(self.click_modifier_right_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.click_modifier_middle = input
                .get_digital_action_data(self.click_modifier_middle_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.scroll = input
                .get_analog_action_data(self.scroll_hnd, hand.input_hnd)
                .map(|x| x.0.y)
                .unwrap_or(0.0);
        }

        let devices = system.get_device_to_absolute_tracking_pose(universe, 0.005);
        app.input_state.hmd = devices[0].mDeviceToAbsoluteTracking.to_affine();
    }

    pub fn update_devices(&mut self, system: &mut SystemManager, app: &mut AppState) {
        app.input_state.devices.clear();
        for i in 0..k_unMaxTrackedDeviceCount {
            let index = TrackedDeviceIndex(i);
            let maybe_role = match system.get_tracked_device_class(index) {
                ETrackedDeviceClass::TrackedDeviceClass_HMD => Some(TrackedDeviceRole::Hmd),
                ETrackedDeviceClass::TrackedDeviceClass_Controller => {
                    let sys_role = system.get_controller_role_for_tracked_device_index(index);
                    match sys_role {
                        ETrackedControllerRole::TrackedControllerRole_LeftHand => {
                            Some(TrackedDeviceRole::LeftHand)
                        }
                        ETrackedControllerRole::TrackedControllerRole_RightHand => {
                            Some(TrackedDeviceRole::RightHand)
                        }
                        _ => None,
                    }
                }
                ETrackedDeviceClass::TrackedDeviceClass_GenericTracker => {
                    Some(TrackedDeviceRole::Tracker)
                }
                _ => None,
            };

            if let Some(role) = maybe_role {
                if let Some(device) = get_tracked_device(system, index, role) {
                    app.input_state.devices.push(device);
                }
            }
        }
        app.input_state.devices.sort_by(|a, b| {
            (a.role as u8)
                .cmp(&(b.role as u8))
                .then(a.index.0.cmp(&b.index.0))
        });
    }
}

fn get_tracked_device(
    system: &mut SystemManager,
    index: TrackedDeviceIndex,
    role: TrackedDeviceRole,
) -> Option<TrackedDevice> {
    let soc = system
        .get_tracked_device_property(
            index,
            ETrackedDeviceProperty::Prop_DeviceBatteryPercentage_Float,
        )
        .ok();

    let charging = if soc.is_some() {
        system
            .get_tracked_device_property(index, ETrackedDeviceProperty::Prop_DeviceIsCharging_Bool)
            .unwrap_or(false)
    } else {
        false
    };

    Some(TrackedDevice {
        valid: true,
        index,
        soc,
        charging,
        role,
    })
}

pub fn set_action_manifest(input: &mut InputManager) -> anyhow::Result<()> {
    let action_path = CONFIG_ROOT_PATH.join("actions.json");

    if !action_path.is_file() {
        File::create(&action_path)
            .unwrap()
            .write_all(include_bytes!("../../res/actions.json"))
            .unwrap();
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_knuckles.json");
    if !binding_path.is_file() {
        File::create(&binding_path)
            .unwrap()
            .write_all(include_bytes!("../../res/actions_binding_knuckles.json"))
            .unwrap();
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_vive.json");
    if !binding_path.is_file() {
        File::create(&binding_path)
            .unwrap()
            .write_all(include_bytes!("../../res/actions_binding_vive.json"))
            .unwrap();
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_oculus.json");
    if !binding_path.is_file() {
        File::create(&binding_path)
            .unwrap()
            .write_all(include_bytes!("../../res/actions_binding_oculus.json"))
            .unwrap();
    }

    if let Err(e) = input.set_action_manifest(action_path.as_path()) {
        bail!("Failed to set action manifest: {}", e.description());
    }
    Ok(())
}
