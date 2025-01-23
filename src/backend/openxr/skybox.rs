use std::{f32::consts::PI, fs::File, sync::Arc};

use glam::{Affine3A, Quat, Vec3A};
use once_cell::sync::Lazy;
use openxr::{self as xr, CompositionLayerFlags};
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};

use crate::{
    backend::openxr::{helpers::translation_rotation_to_posef, swapchain::SwapchainOpts},
    config_io,
    graphics::{dds::WlxCommandBufferDds, format_is_srgb, WlxCommandBuffer},
    state::AppState,
};

use super::{
    swapchain::{create_swapchain_render_data, SwapchainRenderData},
    CompositionLayer, XrState,
};

pub(super) struct Skybox {
    view: Arc<ImageView>,
    srd: Option<(SwapchainRenderData, SwapchainRenderData)>,
}

impl Skybox {
    pub fn new(app: &AppState) -> anyhow::Result<Self> {
        let mut command_buffer = app
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        let mut maybe_image = None;

        'custom_tex: {
            if app.session.config.skybox_texture.is_empty() {
                break 'custom_tex;
            }

            let real_path = config_io::get_config_root().join(&*app.session.config.skybox_texture);
            let Ok(f) = File::open(real_path) else {
                log::warn!(
                    "Could not open custom skybox texture at: {}",
                    app.session.config.skybox_texture
                );
                break 'custom_tex;
            };
            match command_buffer.texture2d_dds(f) {
                Ok(image) => {
                    maybe_image = Some(image);
                }
                Err(e) => {
                    log::warn!(
                        "Could not use custom skybox texture at: {}",
                        app.session.config.skybox_texture
                    );
                    log::warn!("{:?}", e);
                }
            }
        }

        if maybe_image.is_none() {
            let p = include_bytes!("../../res/table_mountain_2.dds");
            maybe_image = Some(command_buffer.texture2d_dds(p.as_slice())?);
        }

        command_buffer.build_and_execute_now()?;

        let view = ImageView::new_default(maybe_image.unwrap())?; // safe unwrap

        Ok(Self { view, srd: None })
    }

    pub(super) fn present_xr<'a>(
        &'a mut self,
        xr: &'a XrState,
        hmd: Affine3A,
        command_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<Vec<CompositionLayer<'a>>> {
        let (sky_image, grid_image) = if let Some((ref mut srd_sky, ref mut srd_grid)) = self.srd {
            (srd_sky.present_last()?, srd_grid.present_last()?)
        } else {
            log::debug!("Render skybox.");

            let mut opts = SwapchainOpts::new().immutable();
            opts.srgb = format_is_srgb(self.view.image().format());

            let srd_sky = create_swapchain_render_data(
                xr,
                command_buffer.graphics.clone(),
                self.view.image().extent(),
                opts,
            )?;

            let srd_grid = create_swapchain_render_data(
                xr,
                command_buffer.graphics.clone(),
                [1024, 1024, 1],
                SwapchainOpts::new().immutable().grid(),
            )?;

            self.srd = Some((srd_sky, srd_grid));

            let (srd_sky, srd_grid) = self.srd.as_mut().unwrap(); // safe unwrap

            (
                srd_sky.acquire_present_release(command_buffer, self.view.clone(), 1.0)?,
                srd_grid.acquire_compute_release(command_buffer)?,
            )
        };

        let pose = xr::Posef {
            orientation: xr::Quaternionf::IDENTITY,
            position: xr::Vector3f {
                x: hmd.translation.x,
                y: hmd.translation.y,
                z: hmd.translation.z,
            },
        };

        // cover the entire sphere
        const HORIZ_ANGLE: f32 = 2.0 * PI;
        const HI_VERT_ANGLE: f32 = 0.5 * PI;
        const LO_VERT_ANGLE: f32 = -0.5 * PI;

        let mut layers = vec![];

        let sky = xr::CompositionLayerEquirect2KHR::new()
            .layer_flags(CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
            .pose(pose)
            .radius(10.0)
            .sub_image(sky_image)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .space(&xr.stage)
            .central_horizontal_angle(HORIZ_ANGLE)
            .upper_vertical_angle(HI_VERT_ANGLE)
            .lower_vertical_angle(LO_VERT_ANGLE);

        layers.push(CompositionLayer::Equirect2(sky));

        static GRID_POSE: Lazy<xr::Posef> = Lazy::new(|| {
            translation_rotation_to_posef(Vec3A::ZERO, Quat::from_rotation_x(PI * -0.5))
        });

        let grid = xr::CompositionLayerQuad::new()
            .layer_flags(CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
            .pose(*GRID_POSE)
            .size(xr::Extent2Df {
                width: 10.0,
                height: 10.0,
            })
            .sub_image(grid_image)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .space(&xr.stage);

        layers.push(CompositionLayer::Quad(grid));

        Ok(layers)
    }
}

pub(super) fn create_skybox(xr: &XrState, app: &AppState) -> Option<Skybox> {
    if !app.session.config.use_skybox {
        return None;
    }
    xr.instance
        .exts()
        .khr_composition_layer_equirect2
        .and_then(|_| Skybox::new(app).ok())
}
