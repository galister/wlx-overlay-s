use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use glam::{Affine3A, Vec3, Vec3A, Vec4};
use idmap::IdMap;
use ovr_overlay::overlay::OverlayManager;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::{ImageAccess, ImageLayout, ImageViewAbstract, ImmutableImage};

use crate::backend::overlay::{OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend};
use crate::graphics::WlxGraphics;
use crate::state::AppState;

use super::overlay::OpenVrOverlayData;

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(1);

pub(super) struct LinePool {
    lines: IdMap<usize, OverlayData<OpenVrOverlayData>>,
    view: Arc<ImageView<ImmutableImage>>,
}

impl LinePool {
    pub fn new(graphics: Arc<WlxGraphics>) -> Self {
        let mut command_buffer = graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit);

        let buf = vec![255; 16];

        let texture = command_buffer.texture2d(2, 2, Format::R8G8B8A8_UNORM, buf);
        command_buffer.build_and_execute_now();

        graphics
            .transition_layout(
                texture.inner().image.clone(),
                ImageLayout::ShaderReadOnlyOptimal,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();

        let view = ImageView::new_default(texture).unwrap();

        LinePool {
            lines: IdMap::new(),
            view,
        }
    }

    pub fn allocate(&mut self, overlay: &mut OverlayManager, app: &mut AppState) -> usize {
        let id = AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed);

        let mut data = OverlayData::<OpenVrOverlayData> {
            state: OverlayState {
                name: Arc::from(format!("wlx-line{}", id)),
                show_hide: true,
                width: 0.002,
                size: (0, 0),
                ..Default::default()
            },
            backend: Box::new(SplitOverlayBackend {
                renderer: Box::new(StaticRenderer {
                    view: self.view.clone(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        data.data.sort_order = 69;

        data.initialize(overlay, app);
        data.upload_texture(overlay, &app.graphics);

        self.lines.insert(id, data);
        id
    }

    pub fn draw_from(&mut self, id: usize, mut from: Affine3A, len: f32, color: Vec4) {
        let rotation = Affine3A::from_axis_angle(Vec3::X, -PI * 0.5);

        from.translation = from.translation + from.transform_vector3a(Vec3A::NEG_Z) * (len * 0.5);
        let transform = from * rotation * Affine3A::from_scale(Vec3::new(1., len / 0.002, 1.));

        self.draw_transform(id, transform, color);
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

    pub fn update(&mut self, overlay: &mut OverlayManager, app: &mut AppState) {
        for data in self.lines.values_mut() {
            data.after_input(overlay, app);
            if data.state.want_visible {
                data.upload_transform(overlay);
                data.upload_color(overlay);
            }
        }
    }
}

struct StaticRenderer {
    view: Arc<ImageView<ImmutableImage>>,
}

impl OverlayRenderer for StaticRenderer {
    fn init(&mut self, _app: &mut AppState) {}
    fn pause(&mut self, _app: &mut AppState) {}
    fn resume(&mut self, _app: &mut AppState) {}
    fn render(&mut self, _app: &mut AppState) {}
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        Some(self.view.clone())
    }
}
