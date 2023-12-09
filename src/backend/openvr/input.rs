use std::{array, io::Write, path::Path};

use glam::Affine3A;
use ovr_overlay::{
    input::{ActionHandle, ActionSetHandle, ActiveActionSet, InputManager, InputValueHandle},
    sys::{
        k_unMaxTrackedDeviceCount, ETrackedControllerRole, ETrackedDeviceClass,
        ETrackedDeviceProperty, ETrackingUniverseOrigin, HmdMatrix34_t,
    },
    system::SystemManager,
    TrackedDeviceIndex,
};

use crate::{
    backend::input::{TrackedDevice, TrackedDeviceRole},
    state::AppState,
};

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
                    copy_from_hmd(&pose.0.pose.mDeviceToAbsoluteTracking, &mut app_hand.pose);
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
        copy_from_hmd(
            &devices[0].mDeviceToAbsoluteTracking,
            &mut app.input_state.hmd,
        );
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

fn copy_from_hmd(in_mat: &HmdMatrix34_t, out_mat: &mut Affine3A) {
    out_mat.x_axis[0] = in_mat.m[0][0];
    out_mat.x_axis[1] = in_mat.m[1][0];
    out_mat.x_axis[2] = in_mat.m[2][0];
    out_mat.y_axis[0] = in_mat.m[0][1];
    out_mat.y_axis[1] = in_mat.m[1][1];
    out_mat.y_axis[2] = in_mat.m[2][1];
    out_mat.z_axis[0] = in_mat.m[0][2];
    out_mat.z_axis[1] = in_mat.m[1][2];
    out_mat.z_axis[2] = in_mat.m[2][2];
    out_mat.w_axis[0] = in_mat.m[0][3];
    out_mat.w_axis[1] = in_mat.m[1][3];
    out_mat.w_axis[2] = in_mat.m[2][3];
}

pub fn action_manifest_path() -> &'static Path {
    let action_path = "/tmp/wlxoverlay-s/actions.json";
    std::fs::create_dir_all("/tmp/wlxoverlay-s").unwrap();

    std::fs::File::create(action_path)
        .unwrap()
        .write_all(include_bytes!("../../res/actions.json"))
        .unwrap();

    std::fs::File::create("/tmp/wlxoverlay-s/actions_binding_knuckles.json")
        .unwrap()
        .write_all(include_bytes!("../../res/actions_binding_knuckles.json"))
        .unwrap();

    std::fs::File::create("/tmp/wlxoverlay-s/actions_binding_vive.json")
        .unwrap()
        .write_all(include_bytes!("../../res/actions_binding_vive.json"))
        .unwrap();

    std::fs::File::create("/tmp/wlxoverlay-s/actions_binding_oculus.json")
        .unwrap()
        .write_all(include_bytes!("../../res/actions_binding_oculus.json"))
        .unwrap();

    Path::new(action_path)
}
