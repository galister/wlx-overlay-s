#[allow(dead_code)]
mod backend;
mod graphics;
mod gui;
mod input;
mod overlays;
mod ovr;
mod shaders;
mod state;

use std::collections::VecDeque;
use std::sync::Arc;

use crate::graphics::{Vert2Uv, WlxGraphics, INDICES};
use crate::input::initialize_input;
use crate::overlays::watch::create_watch;
use crate::{
    shaders::{frag_sprite, vert_common},
    state::AppState,
};
use env_logger::Env;
use log::{info, warn};
use vulkano::{
    buffer::BufferUsage,
    command_buffer::CommandBufferUsage,
    image::{
        view::{ImageView, ImageViewCreateInfo},
        ImageAccess, ImageSubresourceRange, ImageViewType, SwapchainImage,
    },
    pipeline::graphics::viewport::Viewport,
    sampler::Filter,
    swapchain::{
        acquire_next_image, AcquireError, SwapchainCreateInfo, SwapchainCreationError,
        SwapchainPresentInfo,
    },
    sync::{self, FlushError, GpuFuture},
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
    window::Window,
};
use wlx_capture::{frame::WlxFrame, wayland::WlxClient, wlr::WlrDmabufCapture, WlxCapture};

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let (graphics, event_loop) = WlxGraphics::new();
    let (mut swapchain, images) = graphics.create_swapchain(None);

    let mut app = AppState {
        fc: crate::gui::font::FontCache::new(),
        session: crate::state::AppSession::load(),
        tasks: VecDeque::with_capacity(16),
        graphics: graphics.clone(),
        format: swapchain.image_format(),
        input: initialize_input(),
    };

    let wl = WlxClient::new().unwrap();
    let output_id = wl.outputs[0].id;
    let mut capture = WlrDmabufCapture::new(wl, output_id).unwrap();
    let rx = capture.init();

    let vertices = [
        Vert2Uv {
            in_pos: [0., 0.],
            in_uv: [0., 0.],
        },
        Vert2Uv {
            in_pos: [0., 1.],
            in_uv: [0., 1.],
        },
        Vert2Uv {
            in_pos: [1., 0.],
            in_uv: [1., 0.],
        },
        Vert2Uv {
            in_pos: [1., 1.],
            in_uv: [1., 1.],
        },
    ];

    let vertex_buffer = graphics.upload_buffer(BufferUsage::VERTEX_BUFFER, vertices.iter());
    let index_buffer = graphics.upload_buffer(BufferUsage::INDEX_BUFFER, INDICES.iter());

    let vs = vert_common::load(graphics.device.clone()).unwrap();
    let fs = frag_sprite::load(graphics.device.clone()).unwrap();

    let uploads = graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit);

    let mut watch = create_watch(&app, vec![]);
    watch.init(&mut app);
    watch.render(&mut app);

    let pipeline1 = graphics.create_pipeline(vs.clone(), fs.clone(), swapchain.image_format());
    let set1 = pipeline1.uniform_sampler(0, watch.view(), Filter::Nearest);

    capture.request_new_frame();

    let pipeline = graphics.create_pipeline(vs, fs, swapchain.image_format());
    let set0;
    loop {
        if let Ok(frame) = rx.try_recv() {
            match frame {
                WlxFrame::Dmabuf(dmabuf_frame) => match graphics.dmabuf_texture(dmabuf_frame) {
                    Ok(tex) => {
                        let format = tex.format();
                        let view = ImageView::new(
                            tex,
                            ImageViewCreateInfo {
                                format: Some(format),
                                view_type: ImageViewType::Dim2d,
                                subresource_range: ImageSubresourceRange::from_parameters(
                                    format, 1, 1,
                                ),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        set0 = pipeline.uniform_sampler(0, view, Filter::Nearest);
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to create texture from dmabuf: {}", e);
                    }
                },
                _ => {
                    warn!("Received non-dmabuf frame");
                }
            }
        }
    }

    //let set1 = graphics.uniform_buffer(1, vec![1.0, 1.0, 1.0, 1.0]);
    let image_extent_f32 = [
        swapchain.image_extent()[0] as f32,
        swapchain.image_extent()[1] as f32,
    ];
    let image_extent2_f32 = [
        swapchain.image_extent()[0] as f32 / 2.,
        swapchain.image_extent()[1] as f32 / 2.,
    ];
    let pass = pipeline.create_pass(
        image_extent_f32,
        vertex_buffer.clone(),
        index_buffer.clone(),
        vec![set0],
    );
    let pass2 = pipeline1.create_pass(image_extent2_f32, vertex_buffer, index_buffer, vec![set1]);

    let mut viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [1024.0, 1024.0],
        depth_range: 0.0..1.0,
    };

    let mut attachment_image_views = window_size_dependent_setup(&images, &mut viewport);

    //let set1 = pipeline.uniform_buffer(1, vec![1.0, 0.0, 1.0, 1.0]);

    let mut recreate_swapchain = false;
    let mut previous_frame_end = //Some(sync::now(graphics.device.clone()).boxed());
        Some(uploads.end_and_execute().boxed());

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(_),
            ..
        } => {
            recreate_swapchain = true;
        }
        Event::RedrawEventsCleared => {
            previous_frame_end.as_mut().unwrap().cleanup_finished();

            if recreate_swapchain {
                let window = graphics
                    .surface
                    .object()
                    .unwrap()
                    .downcast_ref::<Window>()
                    .unwrap();
                let (new_swapchain, new_images) = match swapchain.recreate(SwapchainCreateInfo {
                    image_extent: window.inner_size().into(),
                    ..swapchain.create_info()
                }) {
                    Ok(r) => r,
                    Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => return,
                    Err(e) => panic!("failed to recreate swapchain: {e}"),
                };

                swapchain = new_swapchain;
                attachment_image_views = window_size_dependent_setup(&new_images, &mut viewport);
                recreate_swapchain = false;
            }

            let (image_index, suboptimal, acquire_future) =
                match acquire_next_image(swapchain.clone(), None) {
                    Ok(r) => r,
                    Err(AcquireError::OutOfDate) => {
                        recreate_swapchain = true;
                        return;
                    }
                    Err(e) => panic!("failed to acquire next image: {e}"),
                };

            if suboptimal {
                recreate_swapchain = true;
            }

            let cmd = graphics
                .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
                .begin(attachment_image_views[image_index as usize].clone())
                .run(&pass)
                .run(&pass2)
                .end_render();

            let future = previous_frame_end
                .take()
                .unwrap()
                .join(acquire_future)
                .then_execute(graphics.queue.clone(), cmd)
                .unwrap()
                .then_swapchain_present(
                    graphics.queue.clone(),
                    SwapchainPresentInfo::swapchain_image_index(swapchain.clone(), image_index),
                )
                .then_signal_fence_and_flush();

            match future {
                Ok(future) => {
                    previous_frame_end = Some(future.boxed());
                }
                Err(FlushError::OutOfDate) => {
                    recreate_swapchain = true;
                    previous_frame_end = Some(sync::now(graphics.device.clone()).boxed());
                }
                Err(e) => {
                    println!("failed to flush future: {e}");
                    previous_frame_end = Some(sync::now(graphics.device.clone()).boxed());
                }
            }
        }
        _ => (),
    });
}

/// This function is called once during initialization, then again whenever the window is resized.
fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage>],
    viewport: &mut Viewport,
) -> Vec<Arc<ImageView<SwapchainImage>>> {
    let dimensions = images[0].dimensions().width_height();
    viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

    images
        .iter()
        .map(|image| ImageView::new_default(image.clone()).unwrap())
        .collect::<Vec<_>>()
}
