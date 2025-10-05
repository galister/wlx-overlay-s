use std::{
    collections::VecDeque,
    ops::Add,
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
use vulkano::{device::physical::PhysicalDevice, Handle, VulkanObject};

use crate::{
    backend::{
        input::interact,
        openvr::{
            helpers::adjust_gain,
            input::{set_action_manifest, OpenVrInputSource},
            lines::LinePool,
            manifest::{install_manifest, uninstall_manifest},
            overlay::OpenVrOverlayData,
        },
        task::{SystemTask, TaskType},
        BackendError,
    },
    graphics::{init_openvr_graphics, CommandBuffers},
    overlays::{
        toast::{Toast, ToastTopic},
        watch::{watch_fade, WATCH_NAME},
    },
    state::AppState,
    subsystem::notifications::NotificationManager,
    windowing::{backend::ShouldRender, manager::OverlayWindowManager, window::OverlayWindowData},
};

#[cfg(feature = "wayvr")]
use crate::{backend::wayvr::WayVRAction, overlays::wayvr::wayvr_action};

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

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn openvr_run(
    running: Arc<AtomicBool>,
    show_by_default: bool,
    headless: bool,
) -> Result<(), BackendError> {
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
        names.iter().map(std::string::String::as_str).collect()
    };

    let mut compositor_mgr = context.compositor_mngr();
    let instance_extensions = {
        let names = compositor_mgr.get_vulkan_instance_extensions_required();
        names.iter().map(std::string::String::as_str).collect()
    };

    let mut app = {
        let (gfx, gfx_extras) = init_openvr_graphics(instance_extensions, device_extensions_fn)?;
        AppState::from_graphics(gfx, gfx_extras)?
    };

    if show_by_default {
        app.tasks.enqueue_at(
            TaskType::System(SystemTask::ShowHide),
            Instant::now().add(Duration::from_secs(1)),
        );
    }

    if let Ok(ipd) = system_mgr.get_tracked_device_property::<f32>(
        TrackedDeviceIndex::HMD,
        ETrackedDeviceProperty::Prop_UserIpdMeters_Float,
    ) {
        app.input_state.ipd = (ipd * 1000.0).round();
        log::info!("IPD: {:.0} mm", app.input_state.ipd);
    }

    let _ = install_manifest(&mut app_mgr);

    let mut overlays = OverlayWindowManager::<OpenVrOverlayData>::new(&mut app, headless)?;
    let mut notifications = NotificationManager::new();
    notifications.run_dbus();
    notifications.run_udp();

    let mut playspace = playspace::PlayspaceMover::new();
    playspace.playspace_changed(&mut compositor_mgr, &mut chaperone_mgr);

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

    log::info!("HMD running @ {refresh_rate} Hz");

    let watch_id = overlays.lookup(WATCH_NAME).unwrap(); // want panic

    // want at least half refresh rate
    let frame_timeout = 2 * (1000.0 / refresh_rate).floor() as u32;

    let mut next_device_update = Instant::now();
    let mut due_tasks = VecDeque::with_capacity(4);

    let mut lines = LinePool::new(app.gfx.clone())?;
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
                EVREventType::VREvent_IpdChanged => {
                    if let Ok(ipd) = system_mgr.get_tracked_device_property::<f32>(
                        TrackedDeviceIndex::HMD,
                        ETrackedDeviceProperty::Prop_UserIpdMeters_Float,
                    ) {
                        let ipd = (ipd * 1000.0).round();
                        if (ipd - app.input_state.ipd).abs() > 0.05 {
                            log::info!("IPD: {:.1} mm -> {:.1} mm", app.input_state.ipd, ipd);
                            Toast::new(ToastTopic::IpdChange, "IPD".into(), format!("{ipd:.1} mm"))
                                .submit(&mut app);
                        }
                        app.input_state.ipd = ipd;
                    }
                }
                _ => {}
            }
        }

        if next_device_update <= Instant::now() {
            input_source.update_devices(&mut system_mgr, &mut app);
            next_device_update = Instant::now() + Duration::from_secs(30);
        }

        notifications.submit_pending(&mut app);

        app.tasks.retrieve_due(&mut due_tasks);

        while let Some(task) = due_tasks.pop_front() {
            match task {
                TaskType::Overlay(sel, f) => {
                    if let Some(o) = overlays.mut_by_selector(&sel) {
                        f(&mut app, &mut o.config);
                    } else {
                        log::warn!("Overlay not found for task: {sel:?}");
                    }
                }
                TaskType::CreateOverlay(sel, f) => {
                    let None = overlays.mut_by_selector(&sel) else {
                        continue;
                    };

                    let Some(overlay_config) = f(&mut app) else {
                        continue;
                    };

                    overlays.add(
                        OverlayWindowData {
                            birthframe: cur_frame,
                            ..OverlayWindowData::from_config(overlay_config)
                        },
                        &mut app,
                    );
                }
                TaskType::DropOverlay(sel) => {
                    if let Some(o) = overlays.mut_by_selector(&sel)
                        && o.birthframe < cur_frame
                    {
                        o.destroy(&mut overlay_mgr);
                        overlays.remove_by_selector(&sel);
                    }
                }
                TaskType::System(task) => match task {
                    SystemTask::ColorGain(channel, value) => {
                        let _ = adjust_gain(&mut settings_mgr, channel, value);
                    }
                    SystemTask::FixFloor => {
                        playspace.fix_floor(&mut chaperone_mgr, &app.input_state);
                    }
                    SystemTask::ResetPlayspace => {
                        playspace.reset_offset(&mut chaperone_mgr, &app.input_state);
                    }
                    SystemTask::ShowHide => {
                        overlays.show_hide(&mut app);
                    }
                },
                TaskType::ToggleSet(set) => {
                    overlays.switch_or_toggle_set(&mut app, set);
                }
                #[cfg(feature = "wayvr")]
                TaskType::WayVR(action) => {
                    wayvr_action(&mut app, &mut overlays, &action);
                }
            }
        }

        let universe = playspace.get_universe();

        app.input_state.pre_update();
        input_source.update(universe.clone(), &mut input_mgr, &mut system_mgr, &mut app);
        app.input_state.post_update(&app.session);

        if app
            .input_state
            .pointers
            .iter()
            .any(|p| p.now.show_hide && !p.before.show_hide)
        {
            lines.mark_dirty(); // workaround to prevent lines from not showing
            overlays.show_hide(&mut app);
        }

        #[cfg(feature = "wayvr")]
        if app
            .input_state
            .pointers
            .iter()
            .any(|p| p.now.toggle_dashboard && !p.before.toggle_dashboard)
        {
            wayvr_action(&mut app, &mut overlays, &WayVRAction::ToggleDashboard);
        }

        overlays
            .values_mut()
            .for_each(|o| o.config.auto_movement(&mut app));

        watch_fade(&mut app, overlays.mut_by_id(watch_id).unwrap()); // want panic
        playspace.update(&mut chaperone_mgr, &mut overlays, &app);

        let lengths_haptics = interact(&mut overlays, &mut app);
        for (idx, (len, haptics)) in lengths_haptics.iter().enumerate() {
            lines.draw_from(
                pointer_lines[idx],
                app.input_state.pointers[idx].pose,
                *len,
                app.input_state.pointers[idx].interaction.mode as usize + 1,
                &app.input_state.hmd,
            );
            if let Some(haptics) = haptics {
                input_source.haptics(&mut input_mgr, idx, haptics);
            }
        }

        app.hid_provider.inner.commit();
        let mut buffers = CommandBuffers::default();

        lines.update(universe.clone(), &mut overlay_mgr, &mut app)?;

        for o in overlays.values_mut() {
            o.after_input(&mut overlay_mgr, &mut app)?;
        }

        #[cfg(feature = "osc")]
        if let Some(ref mut sender) = app.osc_sender {
            let _ = sender.send_params(&overlays, &app.input_state.devices);
        }

        #[cfg(feature = "wayvr")]
        if let Err(e) =
            crate::overlays::wayvr::tick_events::<OpenVrOverlayData>(&mut app, &mut overlays)
        {
            log::error!("WayVR tick_events failed: {e:?}");
        }

        log::trace!("Rendering frame");

        for o in overlays.values_mut() {
            if o.config.active_state.is_some() {
                let ShouldRender::Should = o.should_render(&mut app)? else {
                    continue;
                };
                if !o.ensure_image_allocated(&mut app)? {
                    continue;
                }
                o.data.image_dirty = o.render(
                    &mut app,
                    o.data.image_view.as_ref().unwrap().clone(),
                    &mut buffers,
                    1.0, // alpha is instead set using OVR API
                )?;
            }
        }

        log::trace!("Rendering overlays");

        if let Some(mut future) = buffers.execute_now(app.gfx.queue_gfx.clone())? {
            if let Err(e) = future.flush() {
                return Err(BackendError::Fatal(e.into()));
            }
            future.cleanup_finished();
        }

        overlays
            .values_mut()
            .for_each(|o| o.after_render(universe.clone(), &mut overlay_mgr, &app.gfx));

        #[cfg(feature = "wayvr")]
        if let Some(wayvr) = &app.wayvr {
            wayvr.borrow_mut().data.tick_finish()?;
        }

        // chaperone

        // close font handles?
    }

    log::warn!("OpenVR shutdown");
    // context.shutdown() called by Drop

    Ok(())
}
