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
use libmonado_rs::Monado;
use openxr as xr;
use skybox::create_skybox;
use vulkano::{command_buffer::CommandBufferUsage, Handle, VulkanObject};

use crate::{
    backend::{
        common::{BackendError, OverlayContainer},
        input::interact,
        notifications::NotificationManager,
        openxr::{lines::LinePool, overlay::OpenXrOverlayData},
        overlay::OverlayData,
        task::{SystemTask, TaskType},
    },
    graphics::WlxGraphics,
    overlays::{
        toast::{Toast, ToastTopic},
        watch::{watch_fade, WATCH_NAME},
    },
    state::AppState,
};

#[cfg(feature = "wayvr")]
use crate::overlays::wayvr::wayvr_action;

mod helpers;
mod input;
mod lines;
mod overlay;
mod playspace;
mod skybox;
mod swapchain;

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
const VIEW_COUNT: u32 = 2;
static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct XrState {
    instance: xr::Instance,
    system: xr::SystemId,
    session: xr::Session<xr::Vulkan>,
    predicted_display_time: xr::Time,
    fps: f32,
    stage: Arc<xr::Space>,
    view: Arc<xr::Space>,
    stage_offset: Affine3A,
}

pub fn openxr_run(running: Arc<AtomicBool>, show_by_default: bool) -> Result<(), BackendError> {
    let (xr_instance, system) = match helpers::init_xr() {
        Ok((xr_instance, system)) => (xr_instance, system),
        Err(e) => {
            log::warn!("Will not use OpenXR: {}", e);
            return Err(BackendError::NotSupported);
        }
    };

    let environment_blend_mode = {
        let modes = xr_instance.enumerate_environment_blend_modes(system, VIEW_TYPE)?;
        if modes.contains(&xr::EnvironmentBlendMode::ALPHA_BLEND) {
            xr::EnvironmentBlendMode::ALPHA_BLEND
        } else {
            modes[0]
        }
    };
    log::info!("Using environment blend mode: {:?}", environment_blend_mode);

    let mut app_state = {
        let graphics = WlxGraphics::new_openxr(xr_instance.clone(), system)?;
        AppState::from_graphics(graphics)?
    };

    if show_by_default {
        app_state.tasks.enqueue_at(
            TaskType::System(SystemTask::ShowHide),
            Instant::now().add(Duration::from_secs(1)),
        )
    }

    let mut overlays = OverlayContainer::<OpenXrOverlayData>::new(&mut app_state)?;
    let mut lines = LinePool::new(app_state.graphics.clone())?;

    let mut notifications = NotificationManager::new();
    notifications.run_dbus();
    notifications.run_udp();

    let mut delete_queue = vec![];

    let mut monado = Monado::auto_connect()
        .map_err(|e| log::warn!("Will not use libmonado: {}", e))
        .ok();

    let mut playspace = monado.as_mut().and_then(|m| {
        playspace::PlayspaceMover::new(m)
            .map_err(|e| log::warn!("Will not use Monado playspace mover: {}", e))
            .ok()
    });

    #[cfg(feature = "osc")]
    let mut osc_sender =
        crate::backend::osc::OscSender::new(app_state.session.config.osc_out_port).ok();

    let (session, mut frame_wait, mut frame_stream) = unsafe {
        let raw_session = helpers::create_overlay_session(
            &xr_instance,
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: app_state.graphics.instance.handle().as_raw() as _,
                physical_device: app_state
                    .graphics
                    .device
                    .physical_device()
                    .handle()
                    .as_raw() as _,
                device: app_state.graphics.device.handle().as_raw() as _,
                queue_family_index: app_state.graphics.queue.queue_family_index(),
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
        system,
        session,
        predicted_display_time: xr::Time::from_nanos(0),
        fps: 30.0,
        stage: Arc::new(stage),
        view: Arc::new(view),
        stage_offset: Affine3A::IDENTITY,
    };

    let mut skybox = if environment_blend_mode == xr::EnvironmentBlendMode::OPAQUE {
        create_skybox(&xr_state, &app_state)
    } else {
        None
    };

    let pointer_lines = [
        lines.allocate(&xr_state, app_state.graphics.clone())?,
        lines.allocate(&xr_state, app_state.graphics.clone())?,
    ];

    let watch_id = overlays.get_by_name(WATCH_NAME).unwrap().state.id; // want panic

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
                Ok(_) => log::info!("OpenXR session exit requested."),
                Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break 'main_loop,
                Err(e) => {
                    log::error!("Failed to request OpenXR session exit: {}", e);
                    break 'main_loop;
                }
            }
        }

        while let Some(event) = xr_state.instance.poll_event(&mut event_storage)? {
            use xr::Event::*;
            match event {
                SessionStateChanged(e) => {
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
                InstanceLossPending(_) => {
                    break 'main_loop;
                }
                EventsLost(e) => {
                    log::warn!("lost {} events", e.lost_event_count());
                }
                MainSessionVisibilityChangedEXTX(e) => {
                    if main_session_visible != e.visible() {
                        main_session_visible = e.visible();
                        log::info!("Main session visible: {}", main_session_visible);
                        if main_session_visible {
                            log::debug!("Destroying skybox.");
                            skybox = None;
                        } else if environment_blend_mode == xr::EnvironmentBlendMode::OPAQUE {
                            log::debug!("Allocating skybox.");
                            skybox = create_skybox(&xr_state, &app_state);
                        }
                    }
                }
                _ => {}
            }
        }

        if next_device_update <= Instant::now() {
            if let Some(monado) = &mut monado {
                input_source.update_devices(&mut app_state, monado);
                next_device_update = Instant::now() + Duration::from_secs(30);
            }
        }

        if !session_running {
            std::thread::sleep(Duration::from_millis(100));
            continue 'main_loop;
        }

        let xr_frame_state = frame_wait.wait()?;
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
                .map(|time| time.elapsed().as_secs_f32())
                .unwrap_or(0f32);

            fps_counter.len() as f32 / total_elapsed
        };

        if !xr_frame_state.should_render {
            frame_stream.end(
                xr_frame_state.predicted_display_time,
                environment_blend_mode,
                &[],
            )?;
            continue 'main_loop;
        }

        app_state.input_state.pre_update();
        input_source.update(&xr_state, &mut app_state)?;
        app_state.input_state.post_update(&app_state.session);

        if app_state
            .input_state
            .pointers
            .iter()
            .any(|p| p.now.show_hide && !p.before.show_hide)
        {
            overlays.show_hide(&mut app_state);
        }

        watch_fade(&mut app_state, overlays.mut_by_id(watch_id).unwrap()); // want panic
        if let Some(ref mut space_mover) = playspace {
            space_mover.update(
                &mut overlays,
                &app_state,
                monado.as_mut().unwrap(), // safe
            );
        }

        for o in overlays.iter_mut() {
            o.after_input(&mut app_state)?;
        }

        #[cfg(feature = "osc")]
        if let Some(ref mut sender) = osc_sender {
            let _ = sender.send_params(&overlays, &app_state);
        };

        let (_, views) = xr_state.session.locate_views(
            VIEW_TYPE,
            xr_frame_state.predicted_display_time,
            &xr_state.stage,
        )?;

        let ipd = helpers::ipd_from_views(&views);
        if (app_state.input_state.ipd - ipd).abs() > 0.01 {
            log::info!("IPD changed: {} -> {}", app_state.input_state.ipd, ipd);
            app_state.input_state.ipd = ipd;
            Toast::new(
                ToastTopic::IpdChange,
                "IPD".into(),
                format!("{:.1} mm", ipd).into(),
            )
            .submit(&mut app_state);
        }

        overlays
            .iter_mut()
            .for_each(|o| o.state.auto_movement(&mut app_state));

        let lengths_haptics = interact(&mut overlays, &mut app_state);
        for (idx, (len, haptics)) in lengths_haptics.iter().enumerate() {
            lines.draw_from(
                pointer_lines[idx],
                app_state.input_state.pointers[idx].pose,
                *len,
                app_state.input_state.pointers[idx].interaction.mode as usize + 1,
                &app_state.input_state.hmd,
            );
            if let Some(haptics) = haptics {
                input_source.haptics(&xr_state, idx, haptics);
            }
        }

        app_state.hid_provider.commit();

        let watch = overlays.mut_by_id(watch_id).unwrap(); // want panic
        let watch_transform = watch.state.transform;
        if !watch.state.want_visible {
            watch.state.want_visible = true;
            watch.state.transform = Affine3A::from_scale(Vec3 {
                x: 0.001,
                y: 0.001,
                z: 0.001,
            });
        }

        let mut layers = vec![];
        let mut command_buffer = app_state
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        if !main_session_visible {
            if let Some(skybox) = skybox.as_mut() {
                for (idx, layer) in skybox
                    .present_xr(&xr_state, app_state.input_state.hmd, &mut command_buffer)?
                    .into_iter()
                    .enumerate()
                {
                    layers.push((200.0 - 50.0 * (idx as f32), layer));
                }
            }
        }

        #[cfg(feature = "wayvr")]
        crate::overlays::wayvr::tick_events::<OpenXrOverlayData>(&mut app_state, &mut overlays)?;

        for o in overlays.iter_mut() {
            if !o.state.want_visible {
                continue;
            }

            if !o.data.init {
                o.init(&mut app_state)?;
                o.data.init = true;
            }

            o.render(&mut app_state)?;

            let dist_sq = (app_state.input_state.hmd.translation - o.state.transform.translation)
                .length_squared()
                + (100f32 - o.state.z_order as f32);

            if !dist_sq.is_normal() {
                continue;
            }

            let maybe_layer = o.present_xr(&xr_state, &mut command_buffer)?;
            if let CompositionLayer::None = maybe_layer {
                continue;
            }
            layers.push((dist_sq, maybe_layer));
        }

        for maybe_layer in lines.present_xr(&xr_state, &mut command_buffer)? {
            if let CompositionLayer::None = maybe_layer {
                continue;
            }
            layers.push((0.0, maybe_layer));
        }

        #[cfg(feature = "wayvr")]
        if let Some(wayvr) = &app_state.wayvr {
            wayvr.borrow_mut().state.tick_finish()?;
        }

        command_buffer.build_and_execute_now()?;

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

        frame_stream.end(
            xr_state.predicted_display_time,
            environment_blend_mode,
            &frame_ref,
        )?;

        let removed_overlays = overlays.update(&mut app_state)?;
        for o in removed_overlays {
            delete_queue.push((o, cur_frame + 5));
        }

        notifications.submit_pending(&mut app_state);

        app_state.tasks.retrieve_due(&mut due_tasks);
        while let Some(task) = due_tasks.pop_front() {
            match task {
                TaskType::Global(f) => f(&mut app_state),
                TaskType::Overlay(sel, f) => {
                    if let Some(o) = overlays.mut_by_selector(&sel) {
                        f(&mut app_state, &mut o.state);
                    } else {
                        log::warn!("Overlay not found for task: {:?}", sel);
                    }
                }
                TaskType::CreateOverlay(sel, f) => {
                    let None = overlays.mut_by_selector(&sel) else {
                        continue;
                    };

                    let Some((mut state, backend)) = f(&mut app_state) else {
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
                            if let Some(o) = overlays.remove_by_selector(&sel) {
                                // set for deletion after all images are done showing
                                delete_queue.push((o, cur_frame + 5));
                            }
                        }
                    }
                }
                TaskType::System(task) => match task {
                    SystemTask::FixFloor => {
                        if let Some(ref mut playspace) = playspace {
                            playspace.fix_floor(
                                &app_state.input_state,
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
                        overlays.show_hide(&mut app_state);
                    }
                    _ => {}
                },
                #[cfg(feature = "wayvr")]
                TaskType::WayVR(action) => {
                    wayvr_action(&mut app_state, &mut overlays, &action);
                }
            }
        }

        delete_queue.retain(|(_, frame)| *frame > cur_frame);

        let watch = overlays.mut_by_id(watch_id).unwrap(); // want panic
        watch.state.transform = watch_transform;
    }

    Ok(())
}

pub(super) enum CompositionLayer<'a> {
    None,
    Quad(xr::CompositionLayerQuad<'a, xr::Vulkan>),
    Cylinder(xr::CompositionLayerCylinderKHR<'a, xr::Vulkan>),
    Equirect2(xr::CompositionLayerEquirect2KHR<'a, xr::Vulkan>),
}
