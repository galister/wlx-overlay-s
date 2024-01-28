use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{bail, ensure};
use glam::{Affine3A, Quat, Vec3};
use openxr as xr;
use vulkano::{command_buffer::CommandBufferUsage, Handle, VulkanObject};

use crate::{
    backend::{
        common::OverlayContainer,
        input::interact,
        openxr::{lines::LinePool, overlay::OpenXrOverlayData},
    },
    graphics::WlxGraphics,
    state::AppState,
};

use super::common::BackendError;

mod input;
mod lines;
mod overlay;
mod swapchain;

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
const VIEW_COUNT: u32 = 2;

struct XrState {
    instance: xr::Instance,
    system: xr::SystemId,
    session: xr::Session<xr::Vulkan>,
    predicted_display_time: xr::Time,
    stage: Arc<xr::Space>,
}

pub fn openxr_run(running: Arc<AtomicBool>) -> Result<(), BackendError> {
    let (xr_instance, system) = match init_xr() {
        Ok((xr_instance, system)) => (xr_instance, system),
        Err(e) => {
            log::warn!("Will not use OpenXR: {}", e);
            return Err(BackendError::NotSupported);
        }
    };

    let environment_blend_mode = xr_instance
        .enumerate_environment_blend_modes(system, VIEW_TYPE)
        .unwrap()[0];
    log::info!("Using environment blend mode: {:?}", environment_blend_mode);

    let mut app_state = {
        let graphics = WlxGraphics::new_openxr(xr_instance.clone(), system);
        AppState::from_graphics(graphics)
    };

    let mut overlays = OverlayContainer::<OpenXrOverlayData>::new(&mut app_state);
    let mut lines = LinePool::new(app_state.graphics.clone());

    app_state.hid_provider.set_desktop_extent(overlays.extent);

    let (session, mut frame_wait, mut frame_stream) = unsafe {
        xr_instance
            .create_session::<xr::Vulkan>(
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
            )
            .unwrap()
    };

    let stage = session
        .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)
        .unwrap();

    let mut xr_state = XrState {
        instance: xr_instance,
        system,
        session,
        predicted_display_time: xr::Time::from_nanos(0),
        stage: Arc::new(stage),
    };

    let pointer_lines = [
        lines.allocate(&xr_state, app_state.graphics.clone()),
        lines.allocate(&xr_state, app_state.graphics.clone()),
    ];

    let input_source = input::OpenXrInputSource::new(&xr_state);

    let mut session_running = false;
    let mut event_storage = xr::EventDataBuffer::new();

    'main_loop: loop {
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

        while let Some(event) = xr_state.instance.poll_event(&mut event_storage).unwrap() {
            use xr::Event::*;
            match event {
                SessionStateChanged(e) => {
                    // Session state change is where we can begin and end sessions, as well as
                    // find quit messages!
                    println!("entered state {:?}", e.state());
                    match e.state() {
                        xr::SessionState::READY => {
                            xr_state.session.begin(VIEW_TYPE).unwrap();
                            session_running = true;
                        }
                        xr::SessionState::STOPPING => {
                            xr_state.session.end().unwrap();
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
                    println!("lost {} events", e.lost_event_count());
                }
                _ => {}
            }
        }

        if !session_running {
            std::thread::sleep(Duration::from_millis(100));
            continue 'main_loop;
        }

        let xr_frame_state = frame_wait.wait().unwrap();
        frame_stream.begin().unwrap();

        xr_state.predicted_display_time = xr_frame_state.predicted_display_time;

        if !xr_frame_state.should_render {
            frame_stream
                .end(
                    xr_frame_state.predicted_display_time,
                    environment_blend_mode,
                    &[],
                )
                .unwrap();
            continue 'main_loop;
        }

        app_state.input_state.pre_update();
        input_source.update(&xr_state, &mut app_state);
        app_state.input_state.post_update();

        let (_, views) = xr_state
            .session
            .locate_views(
                VIEW_TYPE,
                xr_frame_state.predicted_display_time,
                &xr_state.stage,
            )
            .unwrap();

        app_state.input_state.hmd = hmd_pose_from_views(&views);

        overlays
            .iter_mut()
            .for_each(|o| o.state.auto_movement(&mut app_state));

        let pointer_lengths = interact(&mut overlays, &mut app_state);
        for (idx, len) in pointer_lengths.iter().enumerate() {
            lines.draw_from(
                pointer_lines[idx],
                app_state.input_state.pointers[idx].pose,
                *len,
                app_state.input_state.pointers[idx].interaction.mode as usize + 1,
            );
        }

        let mut layers = vec![];
        let mut command_buffer = app_state
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit);

        for o in overlays.iter_mut() {
            if !o.state.want_visible {
                continue;
            }

            if !o.data.init {
                o.init(&mut app_state);
                o.data.init = true;
            }

            o.render(&mut app_state);

            if let Some(quad) = o.present_xr(&xr_state, &mut command_buffer) {
                layers.push(quad);
            };
        }

        for quad in lines.present_xr(&xr_state, &mut command_buffer) {
            layers.push(quad);
        }

        command_buffer.build_and_execute_now();

        let frame_ref = layers
            .iter()
            .map(|f| f as &xr::CompositionLayerBase<xr::Vulkan>)
            .collect::<Vec<_>>();

        frame_stream
            .end(
                xr_state.predicted_display_time,
                environment_blend_mode,
                &frame_ref,
            )
            .unwrap();

        app_state.hid_provider.on_new_frame();
    }

    Ok(())
}

fn init_xr() -> Result<(xr::Instance, xr::SystemId), anyhow::Error> {
    let Ok(entry) = (unsafe { xr::Entry::load() }) else {
        bail!("OpenXR Loader not found.");
    };

    let Ok(available_extensions) = entry.enumerate_extensions() else {
        bail!("Failed to enumerate OpenXR extensions.");
    };
    ensure!(
        available_extensions.khr_vulkan_enable2,
        "Missing KHR_vulkan_enable2 extension."
    );
    ensure!(
        available_extensions.extx_overlay,
        "Missing EXTX_overlay extension."
    );

    let mut enabled_extensions = xr::ExtensionSet::default();
    enabled_extensions.khr_vulkan_enable2 = true;
    enabled_extensions.extx_overlay = true;

    //#[cfg(not(debug_assertions))]
    let layers = [];
    //#[cfg(debug_assertions)]
    //let layers = [
    //    "XR_APILAYER_LUNARG_api_dump",
    //    "XR_APILAYER_LUNARG_standard_validation",
    //];

    let Ok(xr_instance) = entry.create_instance(
        &xr::ApplicationInfo {
            application_name: "wlx-overlay-s",
            application_version: 0,
            engine_name: "wlx-overlay-s",
            engine_version: 0,
        },
        &enabled_extensions,
        &layers,
    ) else {
        bail!("Failed to create OpenXR instance.");
    };

    let Ok(instance_props) = xr_instance.properties() else {
        bail!("Failed to query OpenXR instance properties.");
    };
    log::info!(
        "Using OpenXR runtime: {} {}",
        instance_props.runtime_name,
        instance_props.runtime_version
    );

    let Ok(system) = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY) else {
        bail!("Failed to access OpenXR HMD system.");
    };

    let vk_target_version_xr = xr::Version::new(1, 1, 0);

    let Ok(reqs) = xr_instance.graphics_requirements::<xr::Vulkan>(system) else {
        bail!("Failed to query OpenXR Vulkan requirements.");
    };

    if vk_target_version_xr < reqs.min_api_version_supported
        || vk_target_version_xr.major() > reqs.max_api_version_supported.major()
    {
        bail!(
            "OpenXR runtime requires Vulkan version > {}, < {}.0.0",
            reqs.min_api_version_supported,
            reqs.max_api_version_supported.major() + 1
        );
    }

    Ok((xr_instance, system))
}

fn hmd_pose_from_views(views: &Vec<xr::View>) -> Affine3A {
    let pos = {
        let pos0: Vec3 = unsafe { std::mem::transmute(views[0].pose.position) };
        let pos1: Vec3 = unsafe { std::mem::transmute(views[1].pose.position) };
        (pos0 + pos1) * 0.5
    };
    let rot = {
        let rot0 = unsafe { std::mem::transmute(views[0].pose.orientation) };
        let rot1 = unsafe { std::mem::transmute(views[1].pose.orientation) };
        quat_lerp(rot0, rot1, 0.5)
    };

    Affine3A::from_rotation_translation(rot, pos)
}

fn quat_lerp(a: Quat, mut b: Quat, t: f32) -> Quat {
    let l2 = a.dot(b);
    if l2 < 0.0 {
        b = -b;
    }

    Quat::from_xyzw(
        a.x - t * (a.x - b.x),
        a.y - t * (a.y - b.y),
        a.z - t * (a.z - b.z),
        a.w - t * (a.w - b.w),
    )
    .normalize()
}

fn transform_to_posef(transform: &Affine3A) -> xr::Posef {
    let translation = transform.translation;
    let rotation = Quat::from_affine3(transform).normalize();

    xr::Posef {
        orientation: xr::Quaternionf {
            x: rotation.x,
            y: rotation.y,
            z: rotation.z,
            w: rotation.w,
        },
        position: xr::Vector3f {
            x: translation.x,
            y: translation.y,
            z: translation.z,
        },
    }
}
