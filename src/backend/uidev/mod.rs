use std::sync::Arc;

use vulkano::{
    command_buffer::CommandBufferUsage,
    image::{sampler::Filter, view::ImageView, ImageUsage},
    swapchain::{
        acquire_next_image, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo,
    },
    sync::GpuFuture,
    Validated, VulkanError,
};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
    window::Window,
};

use crate::{
    config::load_custom_ui,
    config_io,
    graphics::{DynamicPass, DynamicPipeline, WlxGraphics, BLEND_ALPHA},
    gui::{
        modular::{modular_canvas, ModularData},
        Canvas,
    },
    hid::USE_UINPUT,
    state::AppState,
};

use super::overlay::OverlayRenderer;

static LAST_SIZE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct PreviewState {
    canvas: Canvas<(), ModularData>,
    pipeline: Arc<DynamicPipeline>,
    pass: DynamicPass,
    swapchain: Arc<Swapchain>,
    images: Vec<Arc<ImageView>>,
}

impl PreviewState {
    fn new(
        state: &mut AppState,
        surface: Arc<Surface>,
        window: Arc<Window>,
        panel_name: &str,
    ) -> anyhow::Result<Self> {
        let config = load_custom_ui(panel_name)?;

        let last_size = {
            let size_u64 = LAST_SIZE.load(std::sync::atomic::Ordering::Relaxed);
            [size_u64 as u32, (size_u64 >> 32) as u32]
        };

        if last_size != config.size {
            let logical_size = LogicalSize::new(config.size[0], config.size[1]);
            let _ = window.request_inner_size(logical_size);
            window.set_min_inner_size(Some(logical_size));
            window.set_max_inner_size(Some(logical_size));
            LAST_SIZE.store(
                (config.size[1] as u64) << 32 | config.size[0] as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
        }

        let inner_size = window.inner_size();
        let swapchain_size = [inner_size.width, inner_size.height];
        let (swapchain, images) =
            create_swapchain(&state.graphics, surface.clone(), swapchain_size)?;

        let mut canvas = modular_canvas(&config.size, &config.elements, state)?;
        canvas.init(state)?;
        canvas.render(state).unwrap();
        let view = canvas.view().unwrap();

        let pipeline = {
            let shaders = state.graphics.shared_shaders.read().unwrap();
            state.graphics.create_pipeline_dynamic(
                shaders.get("vert_common").unwrap().clone(), // want panic
                shaders.get("frag_sprite").unwrap().clone(), // want panic
                state.graphics.native_format,
                Some(BLEND_ALPHA),
            )
        }?;
        let set0 = pipeline
            .uniform_sampler(0, view.clone(), Filter::Linear)
            .unwrap();

        let pass = pipeline
            .create_pass(
                [swapchain_size[0] as f32, swapchain_size[1] as f32],
                state.graphics.quad_verts.clone(),
                state.graphics.quad_indices.clone(),
                vec![set0],
            )
            .unwrap();

        Ok(PreviewState {
            canvas,
            pipeline,
            pass,
            swapchain,
            images,
        })
    }
}

pub fn uidev_run(panel_name: &str) -> anyhow::Result<()> {
    let (graphics, event_loop, window, surface) = WlxGraphics::new_window()?;
    window.set_resizable(false);
    window.set_title("WlxOverlay UI Preview");

    USE_UINPUT.store(false, std::sync::atomic::Ordering::Relaxed);

    let mut state = AppState::from_graphics(graphics.clone())?;
    let mut preview = Some(PreviewState::new(
        &mut state,
        surface.clone(),
        window.clone(),
        panel_name,
    )?);
    let mut previous_frame_end = Some(vulkano::sync::now(graphics.device.clone()).boxed());

    let watch_path = config_io::CONFIG_ROOT_PATH.join(format!("{}.yaml", panel_name));
    let mut path_last_modified = watch_path.metadata()?.modified()?;
    let mut recreate = false;

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                elwt.exit();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                recreate = true;
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                previous_frame_end.as_mut().unwrap().cleanup_finished();

                let new_modified = watch_path.metadata().unwrap().modified().unwrap();
                if new_modified > path_last_modified {
                    recreate = true;
                    path_last_modified = new_modified;
                }

                if recreate {
                    drop(preview.take());
                    preview = Some(
                        PreviewState::new(&mut state, surface.clone(), window.clone(), panel_name)
                            .unwrap(),
                    );
                    recreate = false;
                }

                {
                    let preview = preview.as_ref().unwrap();
                    let (image_index, _, acquire_future) =
                        match acquire_next_image(preview.swapchain.clone(), None)
                            .map_err(Validated::unwrap)
                        {
                            Ok(r) => r,
                            Err(VulkanError::OutOfDate) => {
                                recreate = true;
                                return;
                            }
                            Err(e) => panic!("failed to acquire next image: {e}"),
                        };

                    let target = preview.images[image_index as usize].clone();

                    let mut cmd_buf = state
                        .graphics
                        .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
                        .unwrap();
                    cmd_buf.begin_rendering(target).unwrap();
                    let _ = cmd_buf.run_ref(&preview.pass);
                    cmd_buf.end_rendering().unwrap();

                    let command_buffer = cmd_buf.build().unwrap();
                    let future = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .then_execute(graphics.queue.clone(), command_buffer)
                        .unwrap()
                        .then_swapchain_present(
                            graphics.queue.clone(),
                            SwapchainPresentInfo::swapchain_image_index(
                                preview.swapchain.clone(),
                                image_index,
                            ),
                        )
                        .then_signal_fence_and_flush();

                    match future.map_err(Validated::unwrap) {
                        Ok(future) => {
                            previous_frame_end = Some(future.boxed());
                        }
                        Err(VulkanError::OutOfDate) => {
                            previous_frame_end =
                                Some(vulkano::sync::now(state.graphics.device.clone()).boxed());
                        }
                        Err(e) => {
                            println!("failed to flush future: {e}");
                            previous_frame_end =
                                Some(vulkano::sync::now(state.graphics.device.clone()).boxed());
                        }
                    }
                }
            }
            Event::AboutToWait => window.request_redraw(),
            _ => (),
        }
    })?;

    Ok(())
}

fn create_swapchain(
    graphics: &WlxGraphics,
    surface: Arc<Surface>,
    extent: [u32; 2],
) -> anyhow::Result<(Arc<Swapchain>, Vec<Arc<ImageView>>)> {
    let surface_capabilities = graphics
        .device
        .physical_device()
        .surface_capabilities(&surface, Default::default())
        .unwrap();

    let (swapchain, images) = Swapchain::new(
        graphics.device.clone(),
        surface.clone(),
        SwapchainCreateInfo {
            min_image_count: surface_capabilities.min_image_count.max(2),
            image_format: graphics.native_format,
            image_extent: extent,
            image_usage: ImageUsage::COLOR_ATTACHMENT,
            composite_alpha: surface_capabilities
                .supported_composite_alpha
                .into_iter()
                .next()
                .unwrap(),
            ..Default::default()
        },
    )?;

    let image_views = images
        .into_iter()
        .map(|image| ImageView::new_default(image).unwrap())
        .collect::<Vec<_>>();

    Ok((swapchain, image_views))
}
