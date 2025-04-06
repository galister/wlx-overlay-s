use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use glam::{Affine3A, Vec3, Vec3A, Vec4};
use idmap::IdMap;
use ovr_overlay::overlay::OverlayManager;
use ovr_overlay::sys::ETrackingUniverseOrigin;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::ImageLayout;

use crate::backend::overlay::{
    FrameMeta, OverlayData, OverlayRenderer, OverlayState, ShouldRender, SplitOverlayBackend,
    Z_ORDER_LINES,
};
use crate::graphics::{CommandBuffers, WlxGraphics};
use crate::state::AppState;

use super::overlay::OpenVrOverlayData;

static LINE_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(1);

pub(super) struct LinePool {
    lines: IdMap<usize, OverlayData<OpenVrOverlayData>>,
    view: Arc<ImageView>,
    colors: [Vec4; 5],
}

impl LinePool {
    pub fn new(graphics: Arc<WlxGraphics>) -> anyhow::Result<Self> {
        let mut command_buffer =
            graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        let buf = vec![255; 16];

        let texture = command_buffer.texture2d_raw(2, 2, Format::R8G8B8A8_UNORM, &buf)?;
        command_buffer.build_and_execute_now()?;

        graphics
            .transition_layout(
                texture.clone(),
                ImageLayout::ShaderReadOnlyOptimal,
                ImageLayout::TransferSrcOptimal,
            )?
            .wait(None)?;

        let view = ImageView::new_default(texture)?;

        Ok(LinePool {
            lines: IdMap::new(),
            view,
            colors: [
                Vec4::new(1., 1., 1., 1.),
                Vec4::new(0., 0.375, 0.5, 1.),
                Vec4::new(0.69, 0.188, 0., 1.),
                Vec4::new(0.375, 0., 0.5, 1.),
                Vec4::new(1., 0., 0., 1.),
            ],
        })
    }

    pub fn allocate(&mut self) -> usize {
        let id = LINE_AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed);

        let mut data = OverlayData::<OpenVrOverlayData> {
            state: OverlayState {
                name: Arc::from(format!("wlx-line{}", id)),
                show_hide: true,
                ..Default::default()
            },
            backend: Box::new(SplitOverlayBackend {
                renderer: Box::new(StaticRenderer {
                    view: self.view.clone(),
                }),
                ..Default::default()
            }),
            data: OpenVrOverlayData {
                width: 0.002,
                override_width: true,
                ..Default::default()
            },
            ..Default::default()
        };
        data.state.z_order = Z_ORDER_LINES;
        data.state.dirty = true;

        self.lines.insert(id, data);
        id
    }

    pub fn draw_from(
        &mut self,
        id: usize,
        mut from: Affine3A,
        len: f32,
        color: usize,
        hmd: &Affine3A,
    ) {
        let rotation = Affine3A::from_axis_angle(Vec3::X, -PI * 0.5);

        from.translation += from.transform_vector3a(Vec3A::NEG_Z) * (len * 0.5);
        let mut transform = from * rotation * Affine3A::from_scale(Vec3::new(1., len / 0.002, 1.));

        let to_hmd = hmd.translation - from.translation;
        let sides = [Vec3A::Z, Vec3A::X, Vec3A::NEG_Z, Vec3A::NEG_X];
        let rotations = [
            Affine3A::IDENTITY,
            Affine3A::from_axis_angle(Vec3::Y, PI * 0.5),
            Affine3A::from_axis_angle(Vec3::Y, PI * 1.0),
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

        debug_assert!(color < self.colors.len());

        self.draw_transform(id, transform, self.colors[color]);
    }

    fn draw_transform(&mut self, id: usize, transform: Affine3A, color: Vec4) {
        if let Some(data) = self.lines.get_mut(id) {
            data.state.want_visible = true;
            data.state.transform = transform;
            data.data.color = color;
        } else {
            log::warn!("Line {} does not exist", id);
        }
    }

    pub fn hide(&mut self, id: usize) {
        if let Some(data) = self.lines.get_mut(id) {
            data.state.want_visible = false;
        } else {
            log::warn!("Line {} does not exist", id);
        }
    }

    pub fn update(
        &mut self,
        universe: ETrackingUniverseOrigin,
        overlay: &mut OverlayManager,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        for data in self.lines.values_mut() {
            data.after_input(overlay, app)?;
            if data.state.want_visible {
                if data.state.dirty {
                    data.upload_texture(overlay, &app.graphics);
                    data.state.dirty = false;
                }

                data.upload_transform(universe.clone(), overlay);
                data.upload_color(overlay);
            }
        }
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        for data in self.lines.values_mut() {
            data.state.dirty = true;
        }
    }
}

struct StaticRenderer {
    view: Arc<ImageView>,
}

impl OverlayRenderer for StaticRenderer {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, _app: &mut AppState) -> anyhow::Result<ShouldRender> {
        Ok(ShouldRender::Unable)
    }
    fn render(
        &mut self,
        _app: &mut AppState,
        _tgt: Arc<ImageView>,
        _buf: &mut CommandBuffers,
        _alpha: f32,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            extent: self.view.image().extent(),
            ..Default::default()
        })
    }
}
