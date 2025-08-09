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

use vulkano::{command_buffer::CommandBufferUsage, format::Format, image::view::ImageView};

use crate::{
    backend::openxr::helpers,
    graphics::{WlxCommandBuffer, WlxGraphics},
};

use super::{
    swapchain::{create_swapchain_render_data, SwapchainOpts, SwapchainRenderData},
    CompositionLayer, XrState,
};

static LINE_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(1);
pub(super) const LINE_WIDTH: f32 = 0.002;

pub(super) struct LinePool {
    lines: IdMap<usize, LineContainer>,
    colors: Vec<Arc<ImageView>>,
}

impl LinePool {
    pub(super) fn new(graphics: Arc<WlxGraphics>) -> anyhow::Result<Self> {
        let mut command_buffer =
            graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        // TODO customizable colors
        let colors = [
            [0xff, 0xff, 0xff, 0xff],
            [0x00, 0x60, 0x80, 0xff],
            [0xB0, 0x30, 0x00, 0xff],
            [0x60, 0x00, 0x80, 0xff],
            [0xff, 0x00, 0x00, 0xff],
        ];

        let views: anyhow::Result<Vec<Arc<ImageView>>> = colors
            .into_iter()
            .map(
                |color| match command_buffer.texture2d_raw(1, 1, Format::R8G8B8A8_UNORM, &color) {
                    Ok(tex) => ImageView::new_default(tex).map_err(|e| anyhow::anyhow!(e)),
                    Err(e) => Err(e),
                },
            )
            .collect();

        command_buffer.build_and_execute_now()?;

        Ok(LinePool {
            lines: IdMap::new(),
            colors: views?,
        })
    }

    pub(super) fn allocate(
        &mut self,
        xr: &XrState,
        graphics: Arc<WlxGraphics>,
    ) -> anyhow::Result<usize> {
        let id = LINE_AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed);

        let srd =
            create_swapchain_render_data(xr, graphics, [1, 1, 1], SwapchainOpts::new().srgb())?;
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

        debug_assert!(color < self.colors.len());

        let Some(line) = self.lines.get_mut(id) else {
            log::warn!("Line {} not found", id);
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
            view: self.colors[color].clone(),
            pose: posef,
            length: len,
        });
    }

    pub(super) fn present_xr<'a>(
        &'a mut self,
        xr: &'a XrState,
        command_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<Vec<CompositionLayer<'a>>> {
        let mut quads = Vec::new();

        for line in self.lines.values_mut() {
            if let Some(inner) = line.maybe_line.take() {
                let quad = xr::CompositionLayerQuad::new()
                    .pose(inner.pose)
                    .sub_image(line.swapchain.acquire_present_release(
                        command_buffer,
                        inner.view,
                        1.0,
                    )?)
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

    /// the number of lines that are waiting to be drawn
    pub(super) fn num_pending(&self) -> usize {
        self.lines
            .values()
            .filter(|l| l.maybe_line.is_some())
            .count()
    }
}

pub(super) struct Line {
    pub(super) view: Arc<ImageView>,
    pub(super) pose: xr::Posef,
    pub(super) length: f32,
}

struct LineContainer {
    swapchain: SwapchainRenderData,
    maybe_line: Option<Line>,
}
