use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{bail, ensure};
use ash::vk::{self};
use glam::{Affine3A, Quat, Vec3};
use openxr as xr;
use vulkano::{
    image::{view::ImageView, ImageCreateInfo, ImageUsage},
    render_pass::{Framebuffer, FramebufferCreateInfo},
    Handle, VulkanObject,
};

use crate::{
    backend::{common::OverlayContainer, input::interact, openxr::overlay::OpenXrOverlayData},
    graphics::WlxGraphics,
    state::AppState,
};

use super::common::BackendError;

mod input;
mod lines;
mod overlay;

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
const VIEW_COUNT: u32 = 2;

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

    let mut state = {
        let graphics = WlxGraphics::new_xr(xr_instance.clone(), system);
        AppState::from_graphics(graphics)
    };

    let mut overlays = OverlayContainer::<OpenXrOverlayData>::new(&mut state);

    state.hid_provider.set_desktop_extent(overlays.extent);

    let (session, mut frame_wait, mut frame_stream) = unsafe {
        xr_instance
            .create_session::<xr::Vulkan>(
                system,
                &xr::vulkan::SessionCreateInfo {
                    instance: state.graphics.instance.handle().as_raw() as _,
                    physical_device: state.graphics.device.physical_device().handle().as_raw() as _,
                    device: state.graphics.device.handle().as_raw() as _,
                    queue_family_index: state.graphics.queue.queue_family_index(),
                    queue_index: 0,
                },
            )
            .unwrap()
    };

    let input_source = input::OpenXrInputSource::new(session.clone());

    let mut swapchain = None;
    let mut session_running = false;
    let mut event_storage = xr::EventDataBuffer::new();

    'main_loop: loop {
        if !running.load(Ordering::Relaxed) {
            log::warn!("Received shutdown signal.");
            match session.request_exit() {
                Ok(_) => log::info!("OpenXR session exit requested."),
                Err(xr::sys::Result::ERROR_SESSION_NOT_RUNNING) => break 'main_loop,
                Err(e) => {
                    log::error!("Failed to request OpenXR session exit: {}", e);
                    break 'main_loop;
                }
            }
        }

        while let Some(event) = xr_instance.poll_event(&mut event_storage).unwrap() {
            use xr::Event::*;
            match event {
                SessionStateChanged(e) => {
                    // Session state change is where we can begin and end sessions, as well as
                    // find quit messages!
                    println!("entered state {:?}", e.state());
                    match e.state() {
                        xr::SessionState::READY => {
                            session.begin(VIEW_TYPE).unwrap();
                            session_running = true;
                        }
                        xr::SessionState::STOPPING => {
                            session.end().unwrap();
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

        state.input_state.pre_update();
        input_source.update(&session, xr_frame_state.predicted_display_time, &mut state);
        state.input_state.post_update();

        let (_, views) = session
            .locate_views(
                VIEW_TYPE,
                xr_frame_state.predicted_display_time,
                &input_source.stage,
            )
            .unwrap();

        state.input_state.hmd = hmd_pose_from_views(&views);

        let _pointer_lengths = interact(&mut overlays, &mut state);

        //TODO lines

        overlays
            .iter_mut()
            .filter(|o| o.state.want_visible)
            .for_each(|o| o.render(&mut state));

        state.hid_provider.on_new_frame();

        let swapchain = swapchain.get_or_insert_with(|| {
            let views = xr_instance
                .enumerate_view_configuration_views(system, VIEW_TYPE)
                .unwrap();
            debug_assert_eq!(views.len(), VIEW_COUNT as usize);
            debug_assert_eq!(views[0], views[1]);

            let resolution = vk::Extent2D {
                width: views[0].recommended_image_rect_width,
                height: views[0].recommended_image_rect_height,
            };
            log::info!(
                "Swapchain resolution: {}x{}",
                resolution.width,
                resolution.height
            );
            let swapchain = session
                .create_swapchain(&xr::SwapchainCreateInfo {
                    create_flags: xr::SwapchainCreateFlags::EMPTY,
                    usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                        | xr::SwapchainUsageFlags::SAMPLED,
                    format: state.graphics.native_format as _,
                    sample_count: 1,
                    width: resolution.width,
                    height: resolution.height,
                    face_count: 1,
                    array_size: VIEW_COUNT,
                    mip_count: 1,
                })
                .unwrap();

            // thanks @yshui
            let swapchain_images = swapchain
                .enumerate_images()
                .unwrap()
                .into_iter()
                .map(|handle| {
                    let vk_image = vk::Image::from_raw(handle);
                    let raw_image = unsafe {
                        vulkano::image::sys::RawImage::from_handle(
                            state.graphics.device.clone(),
                            vk_image,
                            ImageCreateInfo {
                                format: state.graphics.native_format,
                                extent: [resolution.width * 2, resolution.height, 1],
                                usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_DST,
                                ..Default::default()
                            },
                        )
                        .unwrap()
                    };
                    // SAFETY: OpenXR guarantees that the image is a swapchain image, thus has memory backing it.
                    let image = Arc::new(unsafe { raw_image.assume_bound() });
                    let view = ImageView::new_default(image).unwrap();
                    let fb = Framebuffer::new(
                        todo!(),
                        FramebufferCreateInfo {
                            attachments: vec![view.clone()],
                            ..Default::default()
                        },
                    )
                    .unwrap();

                    XrFramebuffer {
                        framebuffer: fb,
                        color: view,
                    }
                })
                .collect();

            XrSwapchain {
                handle: swapchain,
                buffers: swapchain_images,
                resolution: [resolution.width, resolution.height, 1],
            }
        });

        let image_index = swapchain.handle.acquire_image().unwrap();
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

    #[cfg(not(debug_assertions))]
    let layers = [];
    #[cfg(debug_assertions)]
    let layers = [
        "XR_APILAYER_LUNARG_api_dump",
        "XR_APILAYER_LUNARG_standard_validation",
    ];

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

struct XrSwapchain {
    handle: xr::Swapchain<xr::Vulkan>,
    buffers: Vec<XrFramebuffer>,
    resolution: [u32; 3],
}

struct XrFramebuffer {
    framebuffer: Arc<Framebuffer>,
    color: Arc<ImageView>,
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
