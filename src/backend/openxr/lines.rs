use glam::{Affine3A, Vec3, Vec3A};
use idmap::IdMap;
use openxr as xr;
use std::{
    f32::consts::PI,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use vulkano::command_buffer::CommandBufferUsage;

use crate::{
    backend::openxr::helpers,
    graphics::{CommandBuffers, WlxGraphics, WlxPipeline},
};

use super::{
    swapchain::{create_swapchain, SwapchainOpts, WlxSwapchain},
    CompositionLayer, XrState,
};

static LINE_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(1);
pub(super) const LINE_WIDTH: f32 = 0.002;

// TODO customizable colors
static COLORS: [[f32; 6]; 5] = {
    [
        [1., 1., 1., 1., 0., 0.],
        [0., 0.375, 0.5, 1., 0., 0.],
        [0.69, 0.188, 0., 1., 0., 0.],
        [0.375, 0., 0.5, 1., 0., 0.],
        [1., 0., 0., 1., 0., 0.],
    ]
};

pub(super) struct LinePool {
    lines: IdMap<usize, LineContainer>,
    pipeline: Arc<WlxPipeline>,
}

impl LinePool {
    pub(super) fn new(graphics: Arc<WlxGraphics>) -> anyhow::Result<Self> {
        let Ok(shaders) = graphics.shared_shaders.read() else {
            anyhow::bail!("Failed to lock shared shaders for reading");
        };

        let pipeline = graphics.create_pipeline(
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_color").unwrap().clone(),  // want panic
            graphics.native_format,
            None,
        )?;

        Ok(Self {
            lines: IdMap::new(),
            pipeline,
        })
    }

    pub(super) fn allocate(
        &mut self,
        xr: &XrState,
        graphics: Arc<WlxGraphics>,
    ) -> anyhow::Result<usize> {
        let id = LINE_AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed);

        let srd = create_swapchain(xr, graphics, [1, 1, 1], SwapchainOpts::new())?;
        self.lines.insert(
            id,
            LineContainer {
                swapchain: srd,
                maybe_line: None,
            },
        );
        Ok(id)
    }

    pub(super) fn draw_from(
        &mut self,
        id: usize,
        mut from: Affine3A,
        len: f32,
        color: usize,
        hmd: &Affine3A,
    ) {
        if len < 0.01 {
            return;
        }

        debug_assert!(color < COLORS.len());

        let Some(line) = self.lines.get_mut(id) else {
            log::warn!("Line {id} not found");
            return;
        };

        let rotation = Affine3A::from_axis_angle(Vec3::X, PI * 1.5);

        from.translation += from.transform_vector3a(Vec3A::NEG_Z) * (len * 0.5);
        let mut transform = from * rotation;

        let to_hmd = hmd.translation - from.translation;
        let sides = [Vec3A::Z, Vec3A::X, Vec3A::NEG_Z, Vec3A::NEG_X];
        let rotations = [
            Affine3A::IDENTITY,
            Affine3A::from_axis_angle(Vec3::Y, PI * 0.5),
            Affine3A::from_axis_angle(Vec3::Y, PI * -1.0),
            Affine3A::from_axis_angle(Vec3::Y, PI * 1.5),
        ];
        let mut closest = (0, 0.0);
        for (i, &side) in sides.iter().enumerate() {
            let dot = to_hmd.dot(transform.transform_vector3a(side));
            if i == 0 || dot > closest.1 {
                closest = (i, dot);
            }
        }

        transform *= rotations[closest.0];

        let posef = helpers::transform_to_posef(&transform);

        line.maybe_line = Some(Line {
            color,
            pose: posef,
            length: len,
        });
    }

    pub(super) fn render(
        &mut self,
        graphics: Arc<WlxGraphics>,
        buf: &mut CommandBuffers,
    ) -> anyhow::Result<()> {
        for line in self.lines.values_mut() {
            if let Some(inner) = line.maybe_line.as_mut() {
                let tgt = line.swapchain.acquire_wait_image()?;

                let set0 = self
                    .pipeline
                    .uniform_buffer(0, COLORS[inner.color].to_vec())?;

                let pass = self
                    .pipeline
                    .create_pass_for_target(tgt.clone(), vec![set0])?;

                let mut cmd_buffer =
                    graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
                cmd_buffer.begin_rendering(tgt)?;
                cmd_buffer.run_ref(&pass)?;
                cmd_buffer.end_rendering()?;

                buf.push(cmd_buffer.build()?);
            }
        }

        Ok(())
    }

    pub(super) fn present<'a>(
        &'a mut self,
        xr: &'a XrState,
    ) -> anyhow::Result<Vec<CompositionLayer<'a>>> {
        let mut quads = Vec::new();

        for line in self.lines.values_mut() {
            line.swapchain.ensure_image_released()?;

            if let Some(inner) = line.maybe_line.take() {
                let quad = xr::CompositionLayerQuad::new()
                    .pose(inner.pose)
                    .sub_image(line.swapchain.get_subimage())
                    .eye_visibility(xr::EyeVisibility::BOTH)
                    .space(&xr.stage)
                    .size(xr::Extent2Df {
                        width: LINE_WIDTH,
                        height: inner.length,
                    });

                quads.push(CompositionLayer::Quad(quad));
            }
        }

        Ok(quads)
    }
}

pub(super) struct Line {
    pub(super) color: usize,
    pub(super) pose: xr::Posef,
    pub(super) length: f32,
}

struct LineContainer {
    swapchain: WlxSwapchain,
    maybe_line: Option<Line>,
}
