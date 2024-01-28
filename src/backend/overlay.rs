use std::{
    f32::consts::PI,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use glam::{Affine2, Affine3A, Mat3A, Quat, Vec3, Vec3A};
use vulkano::image::view::ImageView;

use crate::state::AppState;

use super::input::{DummyInteractionHandler, InteractionHandler, PointerHit};

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

pub trait OverlayBackend: OverlayRenderer + InteractionHandler {}

pub struct OverlayState {
    pub id: usize,
    pub name: Arc<str>,
    pub width: f32,
    pub size: (i32, i32),
    pub want_visible: bool,
    pub show_hide: bool,
    pub grabbable: bool,
    pub dirty: bool,
    pub transform: Affine3A,
    pub spawn_point: Vec3A,
    pub spawn_rotation: Quat,
    pub relative_to: RelativeTo,
    pub primary_pointer: Option<usize>,
    pub interaction_transform: Affine2,
}

impl Default for OverlayState {
    fn default() -> Self {
        OverlayState {
            id: AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed),
            name: Arc::from(""),
            width: 1.,
            size: (0, 0),
            want_visible: false,
            show_hide: false,
            grabbable: false,
            dirty: false,
            relative_to: RelativeTo::None,
            spawn_point: Vec3A::NEG_Z,
            spawn_rotation: Quat::IDENTITY,
            transform: Affine3A::IDENTITY,
            primary_pointer: None,
            interaction_transform: Affine2::IDENTITY,
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
            backend: Box::new(SplitOverlayBackend::default()),
            primary_pointer: None,
            data: Default::default(),
        }
    }
}

impl OverlayState {
    pub fn reset(&mut self, _app: &mut AppState) {
        todo!()
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
    pub fn init(&mut self, app: &mut AppState) {
        self.state.transform.translation = app
            .input_state
            .hmd
            .transform_point3a(self.state.spawn_point);
        self.state.realign(&app.input_state.hmd);
        self.backend.init(app);
    }
    pub fn render(&mut self, app: &mut AppState) {
        self.backend.render(app);
    }
    pub fn view(&mut self) -> Option<Arc<ImageView>> {
        self.backend.view()
    }
}

pub trait OverlayRenderer {
    fn init(&mut self, app: &mut AppState);
    fn pause(&mut self, app: &mut AppState);
    fn resume(&mut self, app: &mut AppState);
    fn render(&mut self, app: &mut AppState);
    fn view(&mut self) -> Option<Arc<ImageView>>;
    fn extent(&self) -> [u32; 3];
}

pub struct FallbackRenderer;

impl OverlayRenderer for FallbackRenderer {
    fn init(&mut self, _app: &mut AppState) {}
    fn pause(&mut self, _app: &mut AppState) {}
    fn resume(&mut self, _app: &mut AppState) {}
    fn render(&mut self, _app: &mut AppState) {}
    fn view(&mut self) -> Option<Arc<ImageView>> {
        None
    }
    fn extent(&self) -> [u32; 3] {
        [0, 0, 0]
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

impl OverlayBackend for SplitOverlayBackend {}
impl OverlayRenderer for SplitOverlayBackend {
    fn init(&mut self, app: &mut AppState) {
        self.renderer.init(app);
    }
    fn pause(&mut self, app: &mut AppState) {
        self.renderer.pause(app);
    }
    fn resume(&mut self, app: &mut AppState) {
        self.renderer.resume(app);
    }
    fn render(&mut self, app: &mut AppState) {
        self.renderer.render(app);
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        self.renderer.view()
    }
    fn extent(&self) -> [u32; 3] {
        self.renderer.extent()
    }
}
impl InteractionHandler for SplitOverlayBackend {
    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.interaction.on_left(app, pointer);
    }
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) {
        self.interaction.on_hover(app, hit);
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32) {
        self.interaction.on_scroll(app, hit, delta);
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        self.interaction.on_pointer(app, hit, pressed);
    }
}
