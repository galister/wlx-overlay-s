use glam::Vec4;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use ovr_overlay::{
    sys::{ETrackedDeviceProperty, EVRApplicationType, EVREventType},
    TrackedDeviceIndex,
};
use vulkano::{
    device::{physical::PhysicalDevice, DeviceExtensions},
    instance::InstanceExtensions,
    Handle, VulkanObject,
};

use crate::{
    backend::{
        input::interact,
        openvr::{input::OpenVrInputSource, lines::LinePool},
    },
    state::AppState,
};

use self::{input::action_manifest_path, overlay::OpenVrOverlayData};

use super::common::{OverlayContainer, TaskType};

pub mod input;
pub mod lines;
pub mod overlay;

pub fn openvr_run() {
    let app_type = EVRApplicationType::VRApplication_Overlay;
    let Ok(context) = ovr_overlay::Context::init(app_type) else {
        log::error!("Failed to initialize OpenVR");
        return;
    };

    let mut overlay_mngr = context.overlay_mngr();
    //let mut settings_mngr = context.settings_mngr();
    let mut input_mngr = context.input_mngr();
    let mut system_mngr = context.system_mngr();
    let mut compositor_mngr = context.compositor_mngr();

    let device_extensions_fn = |device: &PhysicalDevice| {
        let names = compositor_mngr.get_vulkan_device_extensions_required(device.handle().as_raw());
        let ext = DeviceExtensions::from_iter(names.iter().map(|s| s.as_str()));
        ext
    };

    let mut compositor_mngr = context.compositor_mngr();
    let instance_extensions = {
        let names = compositor_mngr.get_vulkan_instance_extensions_required();
        InstanceExtensions::from_iter(names.iter().map(|s| s.as_str()))
    };

    let mut state = AppState::new(instance_extensions, device_extensions_fn);
    let mut overlays = OverlayContainer::<OpenVrOverlayData>::new(&mut state);

    state.hid_provider.set_desktop_extent(overlays.extent);

    if let Err(e) = input_mngr.set_action_manifest(action_manifest_path()) {
        log::error!("Failed to set action manifest: {}", e.description());
        return;
    };

    let Ok(mut input_source) = OpenVrInputSource::new(&mut input_mngr) else {
        log::error!("Failed to initialize input");
        return;
    };

    let Ok(refresh_rate) = system_mngr.get_tracked_device_property::<f32>(
        TrackedDeviceIndex::HMD,
        ETrackedDeviceProperty::Prop_DisplayFrequency_Float,
    ) else {
        log::error!("Failed to get display refresh rate");
        return;
    };

    log::info!("HMD running @ {} Hz", refresh_rate);

    let frame_time = (1000.0 / refresh_rate).floor() * 0.001;
    let mut next_device_update = Instant::now();
    let mut due_tasks = VecDeque::with_capacity(4);

    let mut lines = LinePool::new(state.graphics.clone());
    let pointer_lines = [
        lines.allocate(&mut overlay_mngr, &mut state),
        lines.allocate(&mut overlay_mngr, &mut state),
    ];

    loop {
        while let Some(event) = system_mngr.poll_next_event() {
            match event.event_type {
                EVREventType::VREvent_Quit => {
                    log::info!("Received quit event, shutting down.");
                    return;
                }
                EVREventType::VREvent_TrackedDeviceActivated
                | EVREventType::VREvent_TrackedDeviceDeactivated
                | EVREventType::VREvent_TrackedDeviceUpdated => {
                    next_device_update = Instant::now();
                }
                _ => {}
            }
        }

        if next_device_update <= Instant::now() {
            input_source.update_devices(&mut system_mngr, &mut state);
            next_device_update = Instant::now() + Duration::from_secs(30);
        }

        state.tasks.retrieve_due(&mut due_tasks);
        while let Some(task) = due_tasks.pop_front() {
            match task {
                TaskType::Global(f) => f(&mut state),
                TaskType::Overlay(sel, f) => {
                    if let Some(o) = overlays.mut_by_selector(&sel) {
                        f(&mut state, &mut o.state);
                    }
                }
            }
        }

        state.input_state.pre_update();
        input_source.update(&mut input_mngr, &mut system_mngr, &mut state);
        state.input_state.post_update();

        let pointer_lengths = interact(&mut overlays, &mut state);
        for (idx, len) in pointer_lengths.iter().enumerate() {
            lines.draw_from(
                pointer_lines[idx],
                state.input_state.pointers[idx].pose,
                *len,
                Vec4::ONE,
            );
        }

        lines.update(&mut overlay_mngr, &mut state);

        overlays
            .iter_mut()
            .for_each(|o| o.after_input(&mut overlay_mngr, &mut state));

        log::debug!("Rendering frame");

        overlays
            .iter_mut()
            .filter(|o| o.state.want_visible)
            .for_each(|o| o.render(&mut state));

        log::debug!("Rendering overlays");

        overlays
            .iter_mut()
            .for_each(|o| o.after_render(&mut overlay_mngr, &state.graphics));

        // chaperone

        // close font handles?

        // playspace moved end frame

        state.hid_provider.on_new_frame();

        let mut seconds_since_vsync = 0f32;
        std::thread::sleep(Duration::from_secs_f32(
            if system_mngr.get_time_since_last_vsync(&mut seconds_since_vsync, &mut 0u64) {
                frame_time - (seconds_since_vsync % frame_time)
            } else {
                frame_time
            },
        ));
    }
}
