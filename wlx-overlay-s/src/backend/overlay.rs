use std::{
    f32::consts::PI,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use glam::{Affine2, Affine3A, Mat3A, Quat, Vec2, Vec3, Vec3A};
use serde::Deserialize;
use vulkano::{format::Format, image::view::ImageView};

use crate::{
    config::AStrMapExt, graphics::CommandBuffers, state::AppState, subsystem::input::KeyboardFocus,
};

use super::{
    common::snap_upright,
    input::{Haptics, PointerHit},
};

static OVERLAY_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub struct OverlayID(pub usize);

pub const Z_ORDER_TOAST: u32 = 70;
pub const Z_ORDER_LINES: u32 = 69;
pub const Z_ORDER_WATCH: u32 = 68;
pub const Z_ORDER_ANCHOR: u32 = 67;
pub const Z_ORDER_DEFAULT: u32 = 0;
pub const Z_ORDER_DASHBOARD: u32 = Z_ORDER_DEFAULT;

pub struct OverlayState {
    pub id: OverlayID,
    pub name: Arc<str>,
    pub want_visible: bool,
    pub show_hide: bool,
    pub grabbable: bool,
    pub interactable: bool,
    pub recenter: bool,
    pub keyboard_focus: Option<KeyboardFocus>,
    pub dirty: bool,
    pub alpha: f32,
    pub z_order: u32,
    pub transform: Affine3A,
    pub spawn_scale: f32, // aka width
    pub spawn_point: Vec3A,
    pub spawn_rotation: Quat,
    pub saved_transform: Option<Affine3A>,
    pub positioning: Positioning,
    pub curvature: Option<f32>,
    pub birthframe: usize,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            id: OverlayID(OVERLAY_AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed)),
            name: Arc::from(""),
            want_visible: false,
            show_hide: false,
            grabbable: false,
            recenter: false,
            interactable: false,
            keyboard_focus: None,
            dirty: true,
            alpha: 1.0,
            z_order: Z_ORDER_DEFAULT,
            positioning: Positioning::Floating,
            curvature: None,
            spawn_scale: 1.0,
            spawn_point: Vec3A::NEG_Z,
            spawn_rotation: Quat::IDENTITY,
            saved_transform: None,
            transform: Affine3A::IDENTITY,
            birthframe: 0,
        }
    }
}

pub struct OverlayData<T> {
    pub state: OverlayState,
    pub backend: Box<dyn OverlayBackend>,
    pub primary_pointer: Option<usize>,
    pub data: T,
}

impl<T> OverlayData<T>
where
    T: Default,
{
    pub fn from_backend(backend: Box<dyn OverlayBackend>) -> Self {
        Self {
            state: OverlayState::default(),
            backend,
            primary_pointer: None,
            data: T::default(),
        }
    }
}

impl OverlayState {
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
        let (target_transform, lerp) = match self.positioning {
            Positioning::FollowHead { lerp } => (app.input_state.hmd * self.get_transform(), lerp),
            Positioning::FollowHand { hand, lerp } => (
                app.input_state.pointers[hand].pose * self.get_transform(),
                lerp,
            ),
            _ => return,
        };

        self.transform = match lerp {
            1.0 => target_transform,
            lerp => {
                let scale = target_transform.matrix3.x_axis.length();

                let rot_from = Quat::from_mat3a(&self.transform.matrix3.div_scalar(scale));
                let rot_to = Quat::from_mat3a(&target_transform.matrix3.div_scalar(scale));

                let rotation = rot_from.slerp(rot_to, lerp);
                let translation = self
                    .transform
                    .translation
                    .slerp(target_transform.translation, lerp);

                Affine3A::from_scale_rotation_translation(
                    Vec3::ONE * scale,
                    rotation,
                    translation.into(),
                )
            }
        };

        self.dirty = true;
    }

    pub fn reset(&mut self, app: &mut AppState, hard_reset: bool) {
        let parent_transform = match self.positioning {
            Positioning::Floating
            | Positioning::FollowHead { .. }
            | Positioning::FollowHeadPaused { .. } => app.input_state.hmd,
            Positioning::FollowHand { hand, .. } | Positioning::FollowHandPaused { hand, .. } => {
                app.input_state.pointers[hand].pose
            }
            Positioning::Anchored => app.anchor,
            Positioning::FollowOverlay { .. } | Positioning::Static => return,
        };

        if hard_reset {
            self.saved_transform = None;
        }

        self.transform = parent_transform * self.get_transform();

        if self.grabbable && hard_reset {
            self.realign(&app.input_state.hmd);
        }
        self.dirty = true;
    }

    pub fn save_transform(&mut self, app: &mut AppState) -> bool {
        let parent_transform = match self.positioning {
            Positioning::Floating => snap_upright(app.input_state.hmd, Vec3A::Y),
            Positioning::FollowHead { .. } | Positioning::FollowHeadPaused { .. } => {
                app.input_state.hmd
            }
            Positioning::FollowHand { hand, .. } | Positioning::FollowHandPaused { hand, .. } => {
                app.input_state.pointers[hand].pose
            }
            Positioning::Anchored => snap_upright(app.anchor, Vec3A::Y),
            Positioning::FollowOverlay { .. } | Positioning::Static => return false,
        };

        self.saved_transform = Some(parent_transform.inverse() * self.transform);

        true
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
        if self.state.curvature.is_none() {
            self.state.curvature = app
                .session
                .config
                .curve_values
                .arc_get(self.state.name.as_ref())
                .copied();
        }

        if matches!(
            self.state.positioning,
            Positioning::Floating | Positioning::Anchored
        ) {
            let hard_reset = if let Some(transform) = app
                .session
                .config
                .transform_values
                .arc_get(self.state.name.as_ref())
            {
                self.state.saved_transform = Some(*transform);
                false
            } else {
                true
            };
            self.state.reset(app, hard_reset);
        }
        self.backend.init(app)
    }
    pub fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        self.backend.should_render(app)
    }
    pub fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        self.backend.render(app, tgt, buf, alpha)
    }
    pub fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.backend.frame_meta()
    }
}

#[derive(Default, Clone, Copy)]
pub struct FrameMeta {
    pub extent: [u32; 3],
    pub transform: Affine3A,
    pub format: Format,
}

pub enum ShouldRender {
    /// The overlay is dirty and needs to be rendered.
    Should,
    /// The overlay is not dirty but is ready to be rendered.
    Can,
    /// The overlay is not ready to be rendered.
    Unable,
}

pub trait OverlayBackend {
    /// Called once, before the first frame is rendered
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()>;
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()>;

    /// Called when the presentation layer is ready to present a new frame
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender>;

    /// Called when the contents need to be rendered to the swapchain
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool>;

    /// Called to retrieve the effective extent of the image
    /// Used for creating swapchains.
    ///
    /// Must be true if should_render was also true on the same frame.
    fn frame_meta(&mut self) -> Option<FrameMeta>;

    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics>;
    fn on_left(&mut self, app: &mut AppState, pointer: usize);
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool);
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta_y: f32, delta_x: f32);
    fn get_interaction_transform(&mut self) -> Option<Affine2>;
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Positioning {
    /// Stays in place, recenters relative to HMD
    #[default]
    Floating,
    /// Stays in place, recenters relative to anchor
    Anchored,
    /// Following HMD
    FollowHead { lerp: f32 },
    /// Normally follows HMD, but paused due to interaction
    FollowHeadPaused { lerp: f32 },
    /// Following hand
    FollowHand { hand: usize, lerp: f32 },
    /// Normally follows hand, but paused due to interaction
    FollowHandPaused { hand: usize, lerp: f32 },
    /// Follow another overlay
    FollowOverlay { id: usize },
    /// Stays in place, no recentering
    Static,
}

pub fn ui_transform(extent: [u32; 2]) -> Affine2 {
    let aspect = extent[0] as f32 / extent[1] as f32;
    let scale = if aspect < 1.0 {
        Vec2 {
            x: 1.0 / aspect,
            y: -1.0,
        }
    } else {
        Vec2 { x: 1.0, y: -aspect }
    };
    let center = Vec2 { x: 0.5, y: 0.5 };
    Affine2::from_scale_angle_translation(scale, 0.0, center)
}
