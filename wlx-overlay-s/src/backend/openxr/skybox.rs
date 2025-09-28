use std::{
    f32::consts::PI,
    fs::File,
    sync::{Arc, LazyLock},
};

use glam::{Quat, Vec3A};
use openxr as xr;
use vulkano::{
    command_buffer::CommandBufferUsage, image::view::ImageView,
    pipeline::graphics::color_blend::AttachmentBlend,
};
use wgui::gfx::{cmd::WGfxClearMode, pipeline::WPipelineCreateInfo};

use crate::{
    backend::openxr::{helpers::translation_rotation_to_posef, swapchain::SwapchainOpts},
    config_io,
    graphics::{dds::WlxCommandBufferDds, CommandBuffers, ExtentExt},
    state::AppState,
};

use super::{
    swapchain::{create_swapchain, WlxSwapchain},
    CompositionLayer, XrState,
};

pub(super) struct Skybox {
    view: Arc<ImageView>,
    sky: Option<WlxSwapchain>,
    grid: Option<WlxSwapchain>,
}

impl Skybox {
    pub fn new(app: &AppState) -> anyhow::Result<Self> {
        let mut command_buffer = app
            .gfx
            .create_xfer_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

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
            match command_buffer.upload_image_dds(f) {
                Ok(image) => {
                    maybe_image = Some(image);
                }
                Err(e) => {
                    log::warn!(
                        "Could not use custom skybox texture at: {}",
                        app.session.config.skybox_texture
                    );
                    log::warn!("{e:?}");
                }
            }
        }

        if maybe_image.is_none() {
            let p = include_bytes!("../../res/table_mountain_2.dds");
            maybe_image = Some(command_buffer.upload_image_dds(p.as_slice())?);
        }

        command_buffer.build_and_execute_now()?;

        let view = ImageView::new_default(maybe_image.unwrap())?; // safe unwrap

        Ok(Self {
            view,
            sky: None,
            grid: None,
        })
    }

    fn prepare_sky<'a>(
        &'a mut self,
        xr: &'a XrState,
        app: &AppState,
        buf: &mut CommandBuffers,
    ) -> anyhow::Result<()> {
        if self.sky.is_some() {
            return Ok(());
        }
        let opts = SwapchainOpts::new().immutable();

        let extent = self.view.image().extent();
        let mut swapchain = create_swapchain(xr, app.gfx.clone(), extent, opts)?;
        let tgt = swapchain.acquire_wait_image()?;
        let pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_srgb").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format),
        )?;

        let set0 = pipeline.uniform_sampler(0, self.view.clone(), app.gfx.texture_filter)?;
        let set1 = pipeline.uniform_buffer_upload(1, vec![1f32])?;
        let pass = pipeline.create_pass(
            tgt.extent_f32(),
            app.gfx_extras.quad_verts.clone(),
            0..4,
            0..1,
            vec![set0, set1],
            &Default::default(),
        )?;

        let mut cmd_buffer = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_rendering(tgt, WGfxClearMode::DontCare)?;
        cmd_buffer.run_ref(&pass)?;
        cmd_buffer.end_rendering()?;

        buf.push(cmd_buffer.build()?);

        self.sky = Some(swapchain);
        Ok(())
    }

    fn prepare_grid<'a>(
        &'a mut self,
        xr: &'a XrState,
        app: &AppState,
        buf: &mut CommandBuffers,
    ) -> anyhow::Result<()> {
        if self.grid.is_some() {
            return Ok(());
        }

        let extent = [1024, 1024, 1];
        let mut swapchain = create_swapchain(
            xr,
            app.gfx.clone(),
            extent,
            SwapchainOpts::new().immutable(),
        )?;
        let pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_grid").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format).use_blend(AttachmentBlend::alpha()),
        )?;

        let tgt = swapchain.acquire_wait_image()?;
        let pass = pipeline.create_pass(
            tgt.extent_f32(),
            app.gfx_extras.quad_verts.clone(),
            0..4,
            0..1,
            vec![],
            &Default::default(),
        )?;

        let mut cmd_buffer = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_rendering(tgt, WGfxClearMode::Clear([0.0, 0.0, 0.0, 0.0]))?;
        cmd_buffer.run_ref(&pass)?;
        cmd_buffer.end_rendering()?;

        buf.push(cmd_buffer.build()?);

        self.grid = Some(swapchain);
        Ok(())
    }

    pub(super) fn render(
        &mut self,
        xr: &XrState,
        app: &AppState,
        buf: &mut CommandBuffers,
    ) -> anyhow::Result<()> {
        self.prepare_sky(xr, app, buf)?;
        self.prepare_grid(xr, app, buf)?;
        Ok(())
    }

    pub(super) fn present<'a>(
        &'a mut self,
        xr: &'a XrState,
        app: &AppState,
    ) -> anyhow::Result<Vec<CompositionLayer<'a>>> {
        // cover the entire sphere
        const HORIZ_ANGLE: f32 = 2.0 * PI;
        const HI_VERT_ANGLE: f32 = 0.5 * PI;
        const LO_VERT_ANGLE: f32 = -0.5 * PI;

        static GRID_POSE: LazyLock<xr::Posef> = LazyLock::new(|| {
            translation_rotation_to_posef(Vec3A::ZERO, Quat::from_rotation_x(PI * -0.5))
        });

        let pose = xr::Posef {
            orientation: xr::Quaternionf::IDENTITY,
            position: xr::Vector3f {
                x: app.input_state.hmd.translation.x,
                y: app.input_state.hmd.translation.y,
                z: app.input_state.hmd.translation.z,
            },
        };

        self.sky.as_mut().unwrap().ensure_image_released()?;

        let sky = xr::CompositionLayerEquirect2KHR::new()
            .layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
            .pose(pose)
            .radius(10.0)
            .sub_image(self.sky.as_ref().unwrap().get_subimage())
            .eye_visibility(xr::EyeVisibility::BOTH)
            .space(&xr.stage)
            .central_horizontal_angle(HORIZ_ANGLE)
            .upper_vertical_angle(HI_VERT_ANGLE)
            .lower_vertical_angle(LO_VERT_ANGLE);

        self.grid.as_mut().unwrap().ensure_image_released()?;
        let grid = xr::CompositionLayerQuad::new()
            .layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
            .pose(*GRID_POSE)
            .size(xr::Extent2Df {
                width: 10.0,
                height: 10.0,
            })
            .sub_image(self.grid.as_ref().unwrap().get_subimage())
            .eye_visibility(xr::EyeVisibility::BOTH)
            .space(&xr.stage);

        Ok(vec![
            CompositionLayer::Equirect2(sky),
            CompositionLayer::Quad(grid),
        ])
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
