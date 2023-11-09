use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use glam::{Affine3A, Quat, Vec3};
use vulkano::image::ImageViewAbstract;

use crate::state::AppState;

use self::interactions::{DummyInteractionHandler, InteractionHandler, PointerHit};

pub mod interactions;
pub mod keyboard;
pub mod watch;

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

pub enum RelativeTo {
    None,
    Head,
    Hand(usize),
}

pub trait OverlayBackend: OverlayRenderer + InteractionHandler {}

pub struct OverlayData {
    pub id: usize,
    pub name: Arc<str>,
    pub width: f32,
    pub size: (i32, i32),
    pub want_visible: bool,
    pub show_hide: bool,
    pub grabbable: bool,
    pub transform: Affine3A,
    pub spawn_point: Vec3,
    pub spawn_rotation: Quat,
    pub relative_to: RelativeTo,
    pub interaction_transform: Affine3A,
    pub backend: Box<dyn OverlayBackend>,
    pub primary_pointer: Option<usize>,
}
impl Default for OverlayData {
    fn default() -> OverlayData {
        OverlayData {
            id: AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed),
            name: Arc::from(""),
            width: 1.,
            size: (0, 0),
            want_visible: false,
            show_hide: false,
            grabbable: false,
            relative_to: RelativeTo::None,
            spawn_point: Vec3::NEG_Z,
            spawn_rotation: Quat::IDENTITY,
            transform: Affine3A::IDENTITY,
            interaction_transform: Affine3A::IDENTITY,
            backend: Box::new(SplitOverlayBackend::default()),
            primary_pointer: None,
        }
    }
}

impl OverlayData {
    pub fn reset(&mut self, app: &mut AppState) {
        todo!()
    }
    pub fn init(&mut self, app: &mut AppState) {
        self.backend.init(app);
    }
    pub fn render(&mut self, app: &mut AppState) {
        self.backend.render(app);
    }
    pub fn view(&mut self) -> Arc<dyn ImageViewAbstract> {
        self.backend.view()
    }
}

pub trait OverlayRenderer {
    fn init(&mut self, app: &mut AppState);
    fn pause(&mut self, app: &mut AppState);
    fn resume(&mut self, app: &mut AppState);
    fn render(&mut self, app: &mut AppState);
    fn view(&mut self) -> Arc<dyn ImageViewAbstract>;
}

pub struct FallbackRenderer;

impl OverlayRenderer for FallbackRenderer {
    fn init(&mut self, _app: &mut AppState) {}
    fn pause(&mut self, _app: &mut AppState) {}
    fn resume(&mut self, _app: &mut AppState) {}
    fn render(&mut self, _app: &mut AppState) {}
    fn view(&mut self) -> Arc<dyn ImageViewAbstract> {
        unimplemented!()
    }
}
// Boilerplate and dummies

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
    fn view(&mut self) -> Arc<dyn ImageViewAbstract> {
        self.renderer.view()
    }
}
impl InteractionHandler for SplitOverlayBackend {
    fn on_left(&mut self, app: &mut AppState, hand: usize) {
        self.interaction.on_left(app, hand);
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
