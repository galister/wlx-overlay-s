use std::{
    path::Path,
    time::{Duration, Instant},
};

use log::{error, info};
use ovr_overlay::{
    sys::{ETrackedDeviceProperty, EVRApplicationType, EVREventType},
    TrackedDeviceIndex,
};

use super::common::InputState;

pub mod input;
pub mod overlay;

fn openvr_run() {
    let app_type = EVRApplicationType::VRApplication_Overlay;
    let Ok(context) = ovr_overlay::Context::init(app_type) else {
            error!("Failed to initialize OpenVR");
            return;
        };

    let mut overlay = context.overlay_mngr();
    let mut settings = context.settings_mngr();
    let mut input = context.input_mngr();
    let mut system = context.system_mngr();
    let mut compositor = context.compositor_mngr();

    let Ok(_) = input.set_action_manifest(Path::new("resources/actions.json")) else {
        error!("Failed to set action manifest");
        return;
    };

    let Ok(mut input_state) = InputState::new(&mut input) else {
        error!("Failed to initialize input");
        return;
    };

    let Ok(refresh_rate) = system.get_tracked_device_property::<f32>(TrackedDeviceIndex::HMD, ETrackedDeviceProperty::Prop_DisplayFrequency_Float) else {
        error!("Failed to get display refresh rate");
        return;
    };

    let frame_time = (1000.0 / refresh_rate).floor() * 0.001;
    let mut next_device_update = Instant::now();

    loop {
        while let Some(event) = system.poll_next_event() {
            match event.event_type {
                EVREventType::VREvent_Quit => {
                    info!("Received quit event, shutting down.");
                    return;
                }
                EVREventType::VREvent_TrackedDeviceActivated
                | EVREventType::VREvent_TrackedDeviceDeactivated
                | EVREventType::VREvent_TrackedDeviceUpdated => {
                    next_device_update = Instant::now();
                }
                _ => {}
            }

            if next_device_update <= Instant::now() {
                input_state.update_devices(&mut system);
                next_device_update = Instant::now() + Duration::from_secs(30);
            }

            input_state.pre_update();
            input_state.update(&mut input, &mut system);
            input_state.post_update();

            // task scheduler

            // after input

            // interactions

            // show overlays

            // chaperone

            // render overlays

            // hide overlays

            // close font handles

            // playspace moved end frame

            let mut seconds_since_vsync = 0f32;
            std::thread::sleep(Duration::from_secs_f32(
                if system.get_time_since_last_vsync(&mut seconds_since_vsync, &mut 0u64) {
                    frame_time - seconds_since_vsync
                } else {
                    0.011
                },
            ));
        }
    }
}
