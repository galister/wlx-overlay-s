use std::{
    f32::consts::PI,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::Ok;
use glam::{Affine2, Affine3A, Mat3A, Quat, Vec2, Vec3, Vec3A};
use vulkano::image::view::ImageView;

use crate::state::AppState;

use super::{
    common::snap_upright,
    input::{DummyInteractionHandler, Haptics, InteractionHandler, PointerHit},
};

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

pub trait OverlayBackend: OverlayRenderer + InteractionHandler {
    fn set_renderer(&mut self, renderer: Box<dyn OverlayRenderer>);
    fn set_interaction(&mut self, interaction: Box<dyn InteractionHandler>);
}

pub struct OverlayState {
    pub id: usize,
    pub name: Arc<str>,
    pub want_visible: bool,
    pub show_hide: bool,
    pub grabbable: bool,
    pub interactable: bool,
    pub recenter: bool,
    pub dirty: bool,
    pub alpha: f32,
    pub transform: Affine3A,
    pub spawn_scale: f32, // aka width
    pub spawn_point: Vec3A,
    pub spawn_rotation: Quat,
    pub saved_transform: Option<Affine3A>,
    pub relative_to: RelativeTo,
    pub curvature: Option<f32>,
    pub primary_pointer: Option<usize>,
    pub interaction_transform: Affine2,
    pub birthframe: usize,
}

impl Default for OverlayState {
    fn default() -> Self {
        OverlayState {
            id: AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed),
            name: Arc::from(""),
            want_visible: false,
            show_hide: false,
            grabbable: false,
            recenter: false,
            interactable: false,
            dirty: true,
            alpha: 1.0,
            relative_to: RelativeTo::None,
            curvature: None,
            spawn_scale: 1.0,
            spawn_point: Vec3A::NEG_Z,
            spawn_rotation: Quat::IDENTITY,
            saved_transform: None,
            transform: Affine3A::IDENTITY,
            primary_pointer: None,
            interaction_transform: Affine2::IDENTITY,
            birthframe: 0,
        }
    }
}

pub struct OverlayData<T>
where
    T: Default,
{
    pub state: OverlayState,
    pub backend: Box<dyn OverlayBackend>,
    pub primary_pointer: Option<usize>,
    pub data: T,
}

impl<T> Default for OverlayData<T>
where
    T: Default,
{
    fn default() -> Self {
        OverlayData {
            state: Default::default(),
            backend: Box::<SplitOverlayBackend>::default(),
            primary_pointer: None,
            data: Default::default(),
        }
    }
}

impl OverlayState {
    pub fn parent_transform(&self, app: &AppState) -> Option<Affine3A> {
        match self.relative_to {
            RelativeTo::None => None,
            RelativeTo::Head => Some(app.input_state.hmd),
            RelativeTo::Hand(idx) => Some(app.input_state.pointers[idx].pose),
        }
    }

    fn get_transform(&self) -> Affine3A {
        self.saved_transform.unwrap_or_else(|| {
            Affine3A::from_scale_rotation_translation(
                Vec3::ONE * self.spawn_scale,
                self.spawn_rotation,
                self.spawn_point.into(),
            )
        })
    }

    pub fn auto_movement(&mut self, app: &mut AppState) {
        if let Some(parent) = self.parent_transform(app) {
            self.transform = parent * self.get_transform();
            self.dirty = true;
        }
    }

    pub fn reset(&mut self, app: &mut AppState, hard_reset: bool) {
        if hard_reset {
            self.saved_transform = None;
        }

        let hmd = snap_upright(app.input_state.hmd, Vec3A::Y);
        self.transform = hmd * self.get_transform();

        if self.grabbable {
            if hard_reset {
                self.realign(&app.input_state.hmd);
            } else {
                //self.transform = snap_upright(self.transform, app.input_state.hmd.y_axis);
            }
        }
        self.dirty = true;
    }

    pub fn realign(&mut self, hmd: &Affine3A) {
        let to_hmd = hmd.translation - self.transform.translation;
        let up_dir: Vec3A;

        if hmd.x_axis.dot(Vec3A::Y).abs() > 0.2 {
            // Snap upright
            up_dir = hmd.y_axis;
        } else {
            let dot = to_hmd.normalize().dot(hmd.z_axis);
            let z_dist = to_hmd.length();
            let y_dist = (self.transform.translation.y - hmd.translation.y).abs();
            let x_angle = (y_dist / z_dist).asin();

            if dot < -f32::EPSILON {
                // facing down
                let up_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::Y;
                up_dir = (up_point - self.transform.translation).normalize();
            } else if dot > f32::EPSILON {
                // facing up
                let dn_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::NEG_Y;
                up_dir = (self.transform.translation - dn_point).normalize();
            } else {
                // perfectly upright
                up_dir = Vec3A::Y;
            }
        }

        let scale = self.transform.x_axis.length();

        let col_z = (self.transform.translation - hmd.translation).normalize();
        let col_y = up_dir;
        let col_x = col_y.cross(col_z);
        let col_y = col_z.cross(col_x).normalize();
        let col_x = col_x.normalize();

        let rot = Mat3A::from_quat(self.spawn_rotation)
            * Mat3A::from_quat(Quat::from_axis_angle(Vec3::Y, PI));
        self.transform.matrix3 = Mat3A::from_cols(col_x, col_y, col_z).mul_scalar(scale) * rot;
    }
}

impl<T> OverlayData<T>
where
    T: Default,
{
    pub fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.state.reset(app, true);
        self.backend.init(app)
    }
    pub fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.backend.render(app)
    }
    pub fn view(&mut self) -> Option<Arc<ImageView>> {
        self.backend.view()
    }
    pub fn set_visible(&mut self, app: &mut AppState, visible: bool) -> anyhow::Result<()> {
        let old_visible = self.state.want_visible;
        self.state.want_visible = visible;
        if visible != old_visible {
            if visible {
                self.backend.resume(app)?;
            } else {
                self.backend.pause(app)?;
            }
        }
        Ok(())
    }
}

pub trait OverlayRenderer {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn view(&mut self) -> Option<Arc<ImageView>>;
}

pub struct FallbackRenderer;

impl OverlayRenderer for FallbackRenderer {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn render(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        None
    }
}
// Boilerplate and dummies

#[derive(Clone, Copy, Debug, Default)]
pub enum RelativeTo {
    #[default]
    None,
    Head,
    Hand(usize),
}

pub struct SplitOverlayBackend {
    pub renderer: Box<dyn OverlayRenderer>,
    pub interaction: Box<dyn InteractionHandler>,
}

impl Default for SplitOverlayBackend {
    fn default() -> SplitOverlayBackend {
        SplitOverlayBackend {
            renderer: Box::new(FallbackRenderer),
            interaction: Box::new(DummyInteractionHandler),
        }
    }
}

impl OverlayBackend for SplitOverlayBackend {
    fn set_renderer(&mut self, renderer: Box<dyn OverlayRenderer>) {
        self.renderer = renderer;
    }
    fn set_interaction(&mut self, interaction: Box<dyn InteractionHandler>) {
        self.interaction = interaction;
    }
}
impl OverlayRenderer for SplitOverlayBackend {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.renderer.init(app)
    }
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.renderer.pause(app)
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.renderer.resume(app)
    }
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.renderer.render(app)
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        self.renderer.view()
    }
}
impl InteractionHandler for SplitOverlayBackend {
    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.interaction.on_left(app, pointer);
    }
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        self.interaction.on_hover(app, hit)
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32) {
        self.interaction.on_scroll(app, hit, delta);
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        self.interaction.on_pointer(app, hit, pressed);
    }
}

pub fn ui_transform(extent: &[u32; 2]) -> Affine2 {
    let aspect = extent[0] as f32 / extent[1] as f32;
    let scale = if aspect < 1.0 {
        Vec2 {
            x: 1.0 / aspect,
            y: -1.0,
        }
    } else {
        Vec2 {
            x: 1.0,
            y: -1.0 * aspect,
        }
    };
    let center = Vec2 { x: 0.5, y: 0.5 };
    Affine2::from_scale_angle_translation(scale, 0.0, center)
}
