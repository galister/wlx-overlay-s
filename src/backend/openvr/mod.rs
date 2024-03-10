use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
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
        common::SystemTask,
        input::interact,
        notifications::NotificationManager,
        openvr::{
            helpers::adjust_gain,
            input::{set_action_manifest, OpenVrInputSource},
            lines::LinePool,
            manifest::{install_manifest, uninstall_manifest},
            overlay::OpenVrOverlayData,
        },
        overlay::OverlayData,
    },
    graphics::WlxGraphics,
    overlays::watch::{watch_fade, WATCH_NAME},
    state::AppState,
};

use super::common::{BackendError, OverlayContainer, TaskType};

pub mod helpers;
pub mod input;
pub mod lines;
pub mod manifest;
pub mod overlay;
pub mod playspace;

static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn openvr_uninstall() {
    let app_type = EVRApplicationType::VRApplication_Overlay;
    let Ok(context) = ovr_overlay::Context::init(app_type) else {
        log::error!("Uninstall failed: could not reach OpenVR");
        return;
    };

    let mut app_mgr = context.applications_mngr();
    let _ = uninstall_manifest(&mut app_mgr);
}

pub fn openvr_run(running: Arc<AtomicBool>) -> Result<(), BackendError> {
    let app_type = EVRApplicationType::VRApplication_Overlay;
    let Ok(context) = ovr_overlay::Context::init(app_type) else {
        log::warn!("Will not use OpenVR: Context init failed");
        return Err(BackendError::NotSupported);
    };

    log::info!("Using OpenVR runtime");

    let mut app_mgr = context.applications_mngr();
    let mut input_mgr = context.input_mngr();
    let mut system_mgr = context.system_mngr();
    let mut overlay_mgr = context.overlay_mngr();
    let mut settings_mgr = context.settings_mngr();
    let mut chaperone_mgr = context.chaperone_setup_mngr();
    let mut compositor_mgr = context.compositor_mngr();

    let device_extensions_fn = |device: &PhysicalDevice| {
        let names = compositor_mgr.get_vulkan_device_extensions_required(device.handle().as_raw());
        let ext = DeviceExtensions::from_iter(names.iter().map(|s| s.as_str()));
        ext
    };

    let mut compositor_mngr = context.compositor_mngr();
    let instance_extensions = {
        let names = compositor_mngr.get_vulkan_instance_extensions_required();
        InstanceExtensions::from_iter(names.iter().map(|s| s.as_str()))
    };

    let mut state = {
        let graphics = WlxGraphics::new_openvr(instance_extensions, device_extensions_fn)?;
        AppState::from_graphics(graphics)?
    };

    let _ = install_manifest(&mut app_mgr);

    let mut overlays = OverlayContainer::<OpenVrOverlayData>::new(&mut state)?;
    let mut notifications = NotificationManager::new();
    notifications.run_dbus();
    notifications.run_udp();

    let mut playspace = playspace::PlayspaceMover::new();
    playspace.playspace_changed(&mut compositor_mgr, &mut chaperone_mgr);

    #[cfg(feature = "osc")]
    let mut osc_sender =
        crate::backend::osc::OscSender::new(state.session.config.osc_out_port).ok();

    state.hid_provider.set_desktop_extent(overlays.extent);

    set_action_manifest(&mut input_mgr)?;

    let mut input_source = OpenVrInputSource::new(&mut input_mgr)?;

    let Ok(refresh_rate) = system_mgr.get_tracked_device_property::<f32>(
        TrackedDeviceIndex::HMD,
        ETrackedDeviceProperty::Prop_DisplayFrequency_Float,
    ) else {
        return Err(BackendError::Fatal(anyhow!(
            "Failed to get HMD refresh rate"
        )));
    };

    log::info!("HMD running @ {} Hz", refresh_rate);

    let watch_id = overlays.get_by_name(WATCH_NAME).unwrap().state.id; // want panic

    // want at least half refresh rate
    let frame_timeout = 2 * (1000.0 / refresh_rate).floor() as u32;

    let mut next_device_update = Instant::now();
    let mut due_tasks = VecDeque::with_capacity(4);

    let mut lines = LinePool::new(state.graphics.clone())?;
    let pointer_lines = [lines.allocate(), lines.allocate()];

    'main_loop: loop {
        let _ = overlay_mgr.wait_frame_sync(frame_timeout);

        if !running.load(Ordering::Relaxed) {
            log::warn!("Received shutdown signal.");
            break 'main_loop;
        }

        let cur_frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);

        while let Some(event) = system_mgr.poll_next_event() {
            match event.event_type {
                EVREventType::VREvent_Quit => {
                    log::warn!("Received quit event, shutting down.");
                    break 'main_loop;
                }
                EVREventType::VREvent_TrackedDeviceActivated
                | EVREventType::VREvent_TrackedDeviceDeactivated
                | EVREventType::VREvent_TrackedDeviceUpdated => {
                    next_device_update = Instant::now();
                }
                EVREventType::VREvent_SeatedZeroPoseReset
                | EVREventType::VREvent_StandingZeroPoseReset
                | EVREventType::VREvent_ChaperoneUniverseHasChanged
                | EVREventType::VREvent_SceneApplicationChanged => {
                    playspace.playspace_changed(&mut compositor_mgr, &mut chaperone_mgr);
                }
                _ => {}
            }
        }

        if next_device_update <= Instant::now() {
            input_source.update_devices(&mut system_mgr, &mut state);
            next_device_update = Instant::now() + Duration::from_secs(30);
        }

        notifications.submit_pending(&mut state);

        state.tasks.retrieve_due(&mut due_tasks);
        while let Some(task) = due_tasks.pop_front() {
            match task {
                TaskType::Global(f) => f(&mut state),
                TaskType::Overlay(sel, f) => {
                    if let Some(o) = overlays.mut_by_selector(&sel) {
                        f(&mut state, &mut o.state);
                    }
                }
                TaskType::CreateOverlay(sel, f) => {
                    let None = overlays.mut_by_selector(&sel) else {
                        continue;
                    };

                    let Some((mut state, backend)) = f(&mut state) else {
                        continue;
                    };
                    state.birthframe = cur_frame;

                    overlays.add(OverlayData {
                        state,
                        backend,
                        ..Default::default()
                    });
                }
                TaskType::DropOverlay(sel) => {
                    if let Some(o) = overlays.mut_by_selector(&sel) {
                        if o.state.birthframe < cur_frame {
                            o.destroy(&mut overlay_mgr);
                            overlays.remove_by_selector(&sel);
                        }
                    }
                }
                TaskType::System(task) => match task {
                    SystemTask::ColorGain(channel, value) => {
                        let _ = adjust_gain(&mut settings_mgr, channel, value);
                    }
                    SystemTask::FixFloor => {
                        playspace.fix_floor(&mut chaperone_mgr, &state.input_state);
                    }
                    SystemTask::ResetPlayspace => {
                        playspace.reset_offset(&mut chaperone_mgr);
                    }
                },
            }
        }

        let universe = playspace.get_universe();

        state.input_state.pre_update();
        input_source.update(
            universe.clone(),
            &mut input_mgr,
            &mut system_mgr,
            &mut state,
        );
        state.input_state.post_update();

        if state
            .input_state
            .pointers
            .iter()
            .any(|p| p.now.show_hide && !p.before.show_hide)
        {
            overlays.show_hide(&mut state);
        }

        overlays
            .iter_mut()
            .for_each(|o| o.state.auto_movement(&mut state));

        watch_fade(&mut state, overlays.mut_by_id(watch_id).unwrap()); // want panic
        playspace.update(&mut chaperone_mgr, &mut overlays, &state);

        let lengths_haptics = interact(&mut overlays, &mut state);
        for (idx, (len, haptics)) in lengths_haptics.iter().enumerate() {
            lines.draw_from(
                pointer_lines[idx],
                state.input_state.pointers[idx].pose,
                *len,
                state.input_state.pointers[idx].interaction.mode as usize + 1,
                &state.input_state.hmd,
            );
            if let Some(haptics) = haptics {
                input_source.haptics(&mut input_mgr, idx, haptics)
            }
        }

        lines.update(universe.clone(), &mut overlay_mgr, &mut state)?;

        for o in overlays.iter_mut() {
            o.after_input(&mut overlay_mgr, &mut state)?;
        }

        #[cfg(feature = "osc")]
        if let Some(ref mut sender) = osc_sender {
            let _ = sender.send_params(&overlays);
        };

        log::trace!("Rendering frame");

        for o in overlays.iter_mut() {
            if o.state.want_visible {
                o.render(&mut state)?;
            }
        }

        log::trace!("Rendering overlays");

        overlays
            .iter_mut()
            .for_each(|o| o.after_render(universe.clone(), &mut overlay_mgr, &state.graphics));

        // chaperone

        // close font handles?

        state.hid_provider.on_new_frame();
    }

    log::warn!("OpenVR shutdown");
    // context.shutdown() called by Drop

    Ok(())
}
