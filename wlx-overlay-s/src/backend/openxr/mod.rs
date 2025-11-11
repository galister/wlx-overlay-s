use std::{
    collections::VecDeque,
    ops::Add,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use glam::{Affine3A, Vec3};
use input::OpenXrInputSource;
use libmonado::Monado;
use openxr as xr;
use skybox::create_skybox;
use vulkano::{Handle, VulkanObject};

use crate::{
    backend::{
        input::interact,
        openxr::{lines::LinePool, overlay::OpenXrOverlayData},
        task::{SystemTask, TaskType},
        BackendError,
    },
    config::save_state,
    graphics::{init_openxr_graphics, CommandBuffers},
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

mod blocker;
mod helpers;
mod input;
mod lines;
mod overlay;
mod playspace;
mod skybox;
mod swapchain;

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct XrState {
    instance: xr::Instance,
    session: xr::Session<xr::Vulkan>,
    predicted_display_time: xr::Time,
    fps: f32,
    stage: Arc<xr::Space>,
    view: Arc<xr::Space>,
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn openxr_run(
    running: Arc<AtomicBool>,
    show_by_default: bool,
    headless: bool,
) -> Result<(), BackendError> {
    let (xr_instance, system) = match helpers::init_xr() {
        Ok((xr_instance, system)) => (xr_instance, system),
        Err(e) => {
            log::warn!("Will not use OpenXR: {e}");
            return Err(BackendError::NotSupported);
        }
    };

    let mut app = {
        let (gfx, gfx_extras) = init_openxr_graphics(xr_instance.clone(), system)?;
        AppState::from_graphics(gfx, gfx_extras)?
    };

    let environment_blend_mode = {
        let modes = xr_instance.enumerate_environment_blend_modes(system, VIEW_TYPE)?;
        if modes.contains(&xr::EnvironmentBlendMode::ALPHA_BLEND)
            && app.session.config.use_passthrough
        {
            xr::EnvironmentBlendMode::ALPHA_BLEND
        } else {
            modes[0]
        }
    };
    log::info!("Using environment blend mode: {environment_blend_mode:?}");

    if show_by_default {
        app.tasks.enqueue_at(
            TaskType::System(SystemTask::ShowHide),
            Instant::now().add(Duration::from_secs(1)),
        );
    }

    let mut overlays = OverlayWindowManager::<OpenXrOverlayData>::new(&mut app, headless)?;
    let mut lines = LinePool::new(&app)?;

    let mut notifications = NotificationManager::new();
    notifications.run_dbus();
    notifications.run_udp();

    let mut delete_queue = vec![];

    let mut monado = Monado::auto_connect()
        .map_err(|e| log::warn!("Will not use libmonado: {e}"))
        .ok();

    let mut playspace = monado.as_mut().and_then(|m| {
        playspace::PlayspaceMover::new(m)
            .map_err(|e| log::warn!("Will not use Monado playspace mover: {e}"))
            .ok()
    });

    let mut blocker = monado.is_some().then(blocker::InputBlocker::new);

    let (session, mut frame_wait, mut frame_stream) = unsafe {
        let raw_session = helpers::create_overlay_session(
            &xr_instance,
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: app.gfx.instance.handle().as_raw() as _,
                physical_device: app.gfx.device.physical_device().handle().as_raw() as _,
                device: app.gfx.device.handle().as_raw() as _,
                queue_family_index: app.gfx.queue_gfx.queue_family_index(),
                queue_index: 0,
            },
        )?;
        xr::Session::from_raw(xr_instance.clone(), raw_session, Box::new(()))
    };

    let stage =
        session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)?;

    let view = session.create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)?;

    let mut xr_state = XrState {
        instance: xr_instance,
        session,
        predicted_display_time: xr::Time::from_nanos(0),
        fps: 30.0,
        stage: Arc::new(stage),
        view: Arc::new(view),
    };

    let mut skybox = if environment_blend_mode == xr::EnvironmentBlendMode::OPAQUE {
        create_skybox(&xr_state, &app)
    } else {
        None
    };

    let pointer_lines = [
        lines.allocate(&xr_state, app.gfx.clone())?,
        lines.allocate(&xr_state, app.gfx.clone())?,
    ];

    let watch_id = overlays.lookup(WATCH_NAME).unwrap(); // want panic

    let mut input_source = input::OpenXrInputSource::new(&xr_state)?;

    let mut session_running = false;
    let mut event_storage = xr::EventDataBuffer::new();

    let mut next_device_update = Instant::now();
    let mut due_tasks = VecDeque::with_capacity(4);

    let mut fps_counter: VecDeque<Instant> = VecDeque::new();

    let mut main_session_visible = false;

    'main_loop: loop {
        let cur_frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);

        if !running.load(Ordering::Relaxed) {
            log::warn!("Received shutdown signal.");
            match xr_state.session.request_exit() {
                Ok(()) => log::info!("OpenXR session exit requested."),
                Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break 'main_loop,
                Err(e) => {
                    log::error!("Failed to request OpenXR session exit: {e}");
                    break 'main_loop;
                }
            }
        }

        while let Some(event) = xr_state.instance.poll_event(&mut event_storage)? {
            match event {
                xr::Event::SessionStateChanged(e) => {
                    // Session state change is where we can begin and end sessions, as well as
                    // find quit messages!
                    log::info!("entered state {:?}", e.state());
                    match e.state() {
                        xr::SessionState::READY => {
                            xr_state.session.begin(VIEW_TYPE)?;
                            session_running = true;
                        }
                        xr::SessionState::STOPPING => {
                            xr_state.session.end()?;
                            session_running = false;
                        }
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                            break 'main_loop;
                        }
                        _ => {}
                    }
                }
                xr::Event::InstanceLossPending(_) => {
                    break 'main_loop;
                }
                xr::Event::EventsLost(e) => {
                    log::warn!("lost {} events", e.lost_event_count());
                }
                xr::Event::MainSessionVisibilityChangedEXTX(e) => {
                    if main_session_visible != e.visible() {
                        main_session_visible = e.visible();
                        log::info!("Main session visible: {main_session_visible}");
                        if main_session_visible {
                            log::debug!("Destroying skybox.");
                            skybox = None;
                        } else if environment_blend_mode == xr::EnvironmentBlendMode::OPAQUE {
                            log::debug!("Allocating skybox.");
                            skybox = create_skybox(&xr_state, &app);
                        }
                    }
                }
                _ => {}
            }
        }

        if next_device_update <= Instant::now()
            && let Some(monado) = &mut monado
        {
            OpenXrInputSource::update_devices(&mut app, monado);
            next_device_update = Instant::now() + Duration::from_secs(30);
        }

        if !session_running {
            std::thread::sleep(Duration::from_millis(100));
            continue 'main_loop;
        }

        log::trace!("xrWaitFrame");
        let xr_frame_state = frame_wait.wait()?;
        log::trace!("xrBeginFrame");
        frame_stream.begin()?;

        xr_state.predicted_display_time = xr_frame_state.predicted_display_time;
        xr_state.fps = {
            fps_counter.push_back(Instant::now());

            while let Some(time) = fps_counter.front() {
                if time.elapsed().as_secs_f32() > 1. {
                    fps_counter.pop_front();
                } else {
                    break;
                }
            }

            let total_elapsed = fps_counter
                .front()
                .map_or(0f32, |time| time.elapsed().as_secs_f32());

            fps_counter.len() as f32 / total_elapsed
        };

        if !xr_frame_state.should_render {
            log::trace!("xrEndFrame");
            frame_stream.end(
                xr_frame_state.predicted_display_time,
                environment_blend_mode,
                &[],
            )?;
            continue 'main_loop;
        }

        app.input_state.pre_update();
        input_source.update(&xr_state, &mut app)?;
        app.input_state.post_update(&app.session);

        if let Some(ref mut blocker) = blocker {
            blocker.update(
                &app,
                watch_id,
                monado.as_mut().unwrap(), // safe
            );
        }

        if app
            .input_state
            .pointers
            .iter()
            .any(|p| p.now.show_hide && !p.before.show_hide)
        {
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

        watch_fade(&mut app, overlays.mut_by_id(watch_id).unwrap()); // want panic
        if let Some(ref mut space_mover) = playspace {
            space_mover.update(
                &mut overlays,
                &app,
                monado.as_mut().unwrap(), // safe
            );
        }

        for o in overlays.values_mut() {
            o.after_input(&mut app)?;
        }

        #[cfg(feature = "osc")]
        if let Some(ref mut sender) = app.osc_sender {
            let _ = sender.send_params(&overlays, &app.input_state.devices);
        }

        let (_, views) = xr_state.session.locate_views(
            VIEW_TYPE,
            xr_frame_state.predicted_display_time,
            &xr_state.stage,
        )?;

        let ipd = helpers::ipd_from_views(&views);
        if (app.input_state.ipd - ipd).abs() > 0.05 {
            log::info!("IPD changed: {} -> {}", app.input_state.ipd, ipd);
            app.input_state.ipd = ipd;
            Toast::new(ToastTopic::IpdChange, "IPD".into(), format!("{ipd:.1} mm"))
                .submit(&mut app);
        }

        overlays
            .values_mut()
            .for_each(|o| o.config.auto_movement(&mut app));

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
                input_source.haptics(&xr_state, idx, haptics);
            }
        }

        app.hid_provider.inner.commit();

        let watch = overlays.mut_by_id(watch_id).unwrap(); // want panic
        let watch_state = watch.config.active_state.as_mut().unwrap();
        let watch_transform = watch_state.transform;
        if watch_state.alpha < 0.05 {
            //FIXME: Temporary workaround for Monado bug
            watch_state.transform = Affine3A::from_scale(Vec3 {
                x: 0.001,
                y: 0.001,
                z: 0.001,
            });
        }

        #[cfg(feature = "wayvr")]
        if let Err(e) =
            crate::overlays::wayvr::tick_events::<OpenXrOverlayData>(&mut app, &mut overlays)
        {
            log::error!("WayVR tick_events failed: {e:?}");
        }

        // Begin rendering
        let mut buffers = CommandBuffers::default();

        if !main_session_visible && let Some(skybox) = skybox.as_mut() {
            skybox.render(&xr_state, &app, &mut buffers)?;
        }

        for o in overlays.values_mut() {
            o.data.cur_visible = false;
            let Some(alpha) = o.config.active_state.as_ref().map(|x| x.alpha) else {
                continue;
            };

            if !o.data.init {
                o.init(&mut app)?;
                o.data.init = true;
            }

            let should_render = match o.should_render(&mut app)? {
                ShouldRender::Should => true,
                ShouldRender::Can => (o.data.last_alpha - alpha).abs() > f32::EPSILON,
                ShouldRender::Unable => false, //try show old image if exists
            };

            if should_render {
                if !o.ensure_swapchain(&app, &xr_state)? {
                    continue;
                }
                let tgt = o.data.swapchain.as_mut().unwrap().acquire_wait_image()?; // want
                if !o.render(&mut app, tgt, &mut buffers, alpha)? {
                    o.data.swapchain.as_mut().unwrap().ensure_image_released()?; // want
                    continue;
                }
                o.data.last_alpha = alpha;
            } else if o.data.swapchain.is_none() {
                continue;
            }
            o.data.cur_visible = true;
        }

        lines.render(&app, &mut buffers)?;

        let future = buffers.execute_now(app.gfx.queue_gfx.clone())?;
        if let Some(mut future) = future {
            if let Err(e) = future.flush() {
                return Err(BackendError::Fatal(e.into()));
            }
            future.cleanup_finished();
        }
        // End rendering

        // Layer composition
        let mut layers = vec![];
        if !main_session_visible && let Some(skybox) = skybox.as_mut() {
            for (idx, layer) in skybox.present(&xr_state, &app)?.into_iter().enumerate() {
                layers.push(((idx as f32).mul_add(-50.0, 200.0), layer));
            }
        }

        for o in overlays.values_mut() {
            if !o.data.cur_visible {
                continue;
            }
            // unwrap: above if only passes if active_state is some
            let active_state = o.config.active_state.as_ref().unwrap();
            let dist_sq = (app.input_state.hmd.translation - active_state.transform.translation)
                .length_squared()
                + (100f32 - o.config.z_order as f32);
            if !dist_sq.is_normal() {
                o.data.swapchain.as_mut().unwrap().ensure_image_released()?;
                continue;
            }
            let maybe_layer = o.present(&xr_state)?;
            if matches!(maybe_layer, CompositionLayer::None) {
                continue;
            }
            layers.push((dist_sq, maybe_layer));
        }

        for maybe_layer in lines.present(&xr_state)? {
            if matches!(maybe_layer, CompositionLayer::None) {
                continue;
            }
            layers.push((0.0, maybe_layer));
        }
        // End layer composition

        #[cfg(feature = "wayvr")]
        if let Some(wayvr) = &app.wayvr {
            wayvr.borrow_mut().data.tick_finish()?;
        }

        // Begin layer submit
        layers.sort_by(|a, b| b.0.total_cmp(&a.0));

        let frame_ref = layers
            .iter()
            .map(|f| match f.1 {
                CompositionLayer::Quad(ref l) => l as &xr::CompositionLayerBase<xr::Vulkan>,
                CompositionLayer::Cylinder(ref l) => l as &xr::CompositionLayerBase<xr::Vulkan>,
                CompositionLayer::Equirect2(ref l) => l as &xr::CompositionLayerBase<xr::Vulkan>,
                CompositionLayer::None => unreachable!(),
            })
            .collect::<Vec<_>>();

        log::trace!("xrEndFrame");
        frame_stream.end(
            xr_state.predicted_display_time,
            environment_blend_mode,
            &frame_ref,
        )?;
        // End layer submit

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
                        log::debug!("{}: destroy", o.config.name);
                        if let Some(o) = overlays.remove_by_selector(&sel) {
                            // set for deletion after all images are done showing
                            delete_queue.push((o, cur_frame + 5));
                        }
                    }
                }
                TaskType::System(task) => match task {
                    SystemTask::FixFloor => {
                        if let Some(ref mut playspace) = playspace {
                            playspace.fix_floor(
                                &app.input_state,
                                monado.as_mut().unwrap(), // safe
                            );
                        }
                    }
                    SystemTask::ResetPlayspace => {
                        if let Some(ref mut playspace) = playspace {
                            playspace.reset_offset(monado.as_mut().unwrap()); // safe
                        }
                    }
                    SystemTask::ShowHide => {
                        overlays.show_hide(&mut app);
                    }
                    _ => {}
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

        delete_queue.retain(|(_, frame)| *frame > cur_frame);

        //FIXME: Temporary workaround for Monado bug
        let watch = overlays.mut_by_id(watch_id).unwrap(); // want panic
        watch.config.active_state.as_mut().unwrap().transform = watch_transform;
    } // main_loop

    overlays.persist_layout(&mut app);
    if let Err(e) = save_state(&app.session.config) {
        log::error!("Could not save state: {e:?}");
    }

    Ok(())
}

pub(super) enum CompositionLayer<'a> {
    None,
    Quad(xr::CompositionLayerQuad<'a, xr::Vulkan>),
    Cylinder(xr::CompositionLayerCylinderKHR<'a, xr::Vulkan>),
    Equirect2(xr::CompositionLayerEquirect2KHR<'a, xr::Vulkan>),
}
