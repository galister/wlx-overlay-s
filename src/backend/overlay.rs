use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use glam::{Affine3A, Quat, Vec3A};
use vulkano::image::ImageViewAbstract;

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
    pub transform: Affine3A,
    pub spawn_point: Vec3A,
    pub spawn_rotation: Quat,
    pub relative_to: RelativeTo,
    pub primary_pointer: Option<usize>,
    pub interaction_transform: Affine3A,
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
            relative_to: RelativeTo::None,
            spawn_point: Vec3A::NEG_Z,
            spawn_rotation: Quat::IDENTITY,
            transform: Affine3A::IDENTITY,
            primary_pointer: None,
            interaction_transform: Affine3A::IDENTITY,
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
}

impl<T> OverlayData<T>
where
    T: Default,
{
    pub fn init(&mut self, app: &mut AppState) {
        self.backend.init(app);
    }
    pub fn render(&mut self, app: &mut AppState) {
        self.backend.render(app);
    }
    pub fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        self.backend.view()
    }
}

pub trait OverlayRenderer {
    fn init(&mut self, app: &mut AppState);
    fn pause(&mut self, app: &mut AppState);
    fn resume(&mut self, app: &mut AppState);
    fn render(&mut self, app: &mut AppState);
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>>;
}

pub struct FallbackRenderer;

impl OverlayRenderer for FallbackRenderer {
    fn init(&mut self, _app: &mut AppState) {}
    fn pause(&mut self, _app: &mut AppState) {}
    fn resume(&mut self, _app: &mut AppState) {}
    fn render(&mut self, _app: &mut AppState) {}
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        unimplemented!()
    }
}
// Boilerplate and dummies

pub enum RelativeTo {
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
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        self.renderer.view()
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
