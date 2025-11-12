use glam::{Affine3A, Mat3A, Quat, Vec3, Vec3A};
use serde::{Deserialize, Serialize};
use std::{f32::consts::PI, sync::Arc};

use crate::{
    state::{AppState, LeftRight},
    subsystem::input::KeyboardFocus,
    windowing::{
        backend::{FrameMeta, OverlayBackend, RenderResources, ShouldRender},
        snap_upright,
    },
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum Positioning {
    /// Stays in place, recenters relative to HMD
    #[default]
    Floating,
    /// Stays in place, recenters relative to anchor
    Anchored,
    /// Stays in place, no recentering
    Static,
    /// Following HMD
    FollowHead { lerp: f32 },
    /// Normally follows HMD, but paused due to interaction
    FollowHeadPaused { lerp: f32 },
    /// Following hand
    FollowHand { hand: LeftRight, lerp: f32 },
    /// Normally follows hand, but paused due to interaction
    FollowHandPaused { hand: LeftRight, lerp: f32 },
}

impl Positioning {
    pub const fn moves_with_space(&self) -> bool {
        matches!(self, Self::Floating | Self::Anchored | Self::Static)
    }
}

pub struct OverlayWindowData<T> {
    pub config: OverlayWindowConfig,
    pub data: T,
    pub birthframe: usize,
    pub primary_pointer: Option<usize>,
}

impl<T> OverlayWindowData<T>
where
    T: Default,
{
    pub fn from_config(config: OverlayWindowConfig) -> Self {
        Self {
            data: T::default(),
            config,
            primary_pointer: None,
            birthframe: 0,
        }
    }
}

impl<T> OverlayWindowData<T> {
    pub fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        //TODO: load state?

        self.config.backend.init(app)
    }
    pub fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        self.config.backend.should_render(app)
    }
    pub fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()> {
        self.config.backend.render(app, rdr)
    }
    pub fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.config.backend.frame_meta()
    }
}

pub struct OverlayWindowConfig {
    pub name: Arc<str>,
    pub backend: Box<dyn OverlayBackend>,
    /// The default state to show when the overlay is newly spawned.
    pub default_state: OverlayWindowState,
    /// The current state to show. None if the overlay is hidden.
    pub active_state: Option<OverlayWindowState>,
    /// Order to draw overlays in. Overlays with higher numbers will be drawn over ones with lower numbers.
    pub z_order: u32,
    /// If set, hovering this overlay will cause the HID provider to switch focus.
    pub keyboard_focus: Option<KeyboardFocus>,
    /// Should the overlay be displayed on the next frame?
    pub show_on_spawn: bool,
    /// Does not belong to any set; switching sets does not affect this overlay.
    pub global: bool,
    /// True if transform, curvature, alpha has changed. Only used by OpenVR.
    pub dirty: bool,
    /// True if the window is showing the edit overlay
    pub editing: bool,
    pub saved_transform: Option<Affine3A>,
}

impl OverlayWindowConfig {
    pub fn from_backend(backend: Box<dyn OverlayBackend>) -> Self {
        Self {
            name: "".into(),
            backend,
            default_state: OverlayWindowState {
                transform: Affine3A::from_translation(Vec3::NEG_Z),
                ..OverlayWindowState::default()
            },
            active_state: None,
            saved_transform: None,
            z_order: 0,
            keyboard_focus: None,
            show_on_spawn: false,
            global: false,
            dirty: true,
            editing: false,
        }
    }

    pub fn activate(&mut self, app: &mut AppState) {
        log::debug!("activate {}", self.name.as_ref());
        self.dirty = true;
        self.active_state = Some(self.default_state.clone());
        self.reset(app, true);
    }

    pub fn activate_static(&mut self, global_transform: Affine3A) {
        log::debug!("activate {}", self.name.as_ref());
        self.dirty = true;
        let mut state = self.default_state.clone();
        state.transform = global_transform;
        self.active_state = Some(state);
    }

    pub fn deactivate(&mut self) {
        log::debug!("deactivate {}", self.name.as_ref());
        self.active_state = None;
    }

    pub fn toggle(&mut self, app: &mut AppState) {
        if self.active_state.take().is_none() {
            self.activate(app);
        } else {
            log::debug!("deactivate {}", self.name.as_ref());
        }
    }

    pub fn auto_movement(&mut self, app: &mut AppState) {
        let Some(state) = self.active_state.as_mut() else {
            return;
        };

        let cur_transform = self.saved_transform.unwrap_or(self.default_state.transform);

        let (target_transform, lerp) = match state.positioning {
            Positioning::FollowHead { lerp } => (app.input_state.hmd * cur_transform, lerp),
            Positioning::FollowHand { hand, lerp } => (
                app.input_state.pointers[hand as usize].pose * cur_transform,
                lerp,
            ),
            _ => return,
        };

        state.transform = match lerp {
            1.0 => target_transform,
            lerp => {
                let scale = target_transform.matrix3.x_axis.length();

                let rot_from = Quat::from_mat3a(&state.transform.matrix3.div_scalar(scale));
                let rot_to = Quat::from_mat3a(&target_transform.matrix3.div_scalar(scale));

                let rotation = rot_from.slerp(rot_to, lerp);
                let translation = state
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

    /// Returns true if changes were saved.
    pub fn save_transform(&mut self, app: &mut AppState) -> bool {
        let Some(state) = self.active_state.as_mut() else {
            return false;
        };

        let parent_transform = match state.positioning {
            Positioning::Floating => snap_upright(app.input_state.hmd, Vec3A::Y),
            Positioning::FollowHead { .. } | Positioning::FollowHeadPaused { .. } => {
                app.input_state.hmd
            }
            Positioning::FollowHand { hand, .. } | Positioning::FollowHandPaused { hand, .. } => {
                app.input_state.pointers[hand as usize].pose
            }
            Positioning::Anchored => snap_upright(app.anchor, Vec3A::Y),
            Positioning::Static => return false,
        };

        self.saved_transform = Some(parent_transform.inverse() * state.transform);

        true
    }

    pub fn reset(&mut self, app: &mut AppState, hard_reset: bool) {
        let Some(state) = self.active_state.as_mut() else {
            return;
        };

        let cur_transform = self.saved_transform.unwrap_or(self.default_state.transform);

        let parent_transform = match state.positioning {
            Positioning::Floating
            | Positioning::FollowHead { .. }
            | Positioning::FollowHeadPaused { .. } => app.input_state.hmd,
            Positioning::FollowHand { hand, .. } | Positioning::FollowHandPaused { hand, .. } => {
                app.input_state.pointers[hand as usize].pose
            }
            Positioning::Anchored => app.anchor,
            Positioning::Static => return,
        };

        if hard_reset {
            self.saved_transform = None;
        }

        state.transform = parent_transform * cur_transform;

        if state.grabbable && hard_reset {
            self.realign(&app.input_state.hmd);
        }
        self.dirty = true;
    }

    pub fn realign(&mut self, hmd: &Affine3A) {
        let Some(state) = self.active_state.as_mut() else {
            return;
        };

        let to_hmd = hmd.translation - state.transform.translation;
        let up_dir: Vec3A;

        if hmd.x_axis.dot(Vec3A::Y).abs() > 0.2 {
            // Snap upright
            up_dir = hmd.y_axis;
        } else {
            let dot = to_hmd.normalize().dot(hmd.z_axis);
            let z_dist = to_hmd.length();
            let y_dist = (state.transform.translation.y - hmd.translation.y).abs();
            let x_angle = (y_dist / z_dist).asin();

            if dot < -f32::EPSILON {
                // facing down
                let up_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::Y;
                up_dir = (up_point - state.transform.translation).normalize();
            } else if dot > f32::EPSILON {
                // facing up
                let dn_point = hmd.translation + z_dist / x_angle.cos() * Vec3A::NEG_Y;
                up_dir = (state.transform.translation - dn_point).normalize();
            } else {
                // perfectly upright
                up_dir = Vec3A::Y;
            }
        }

        let scale = state.transform.x_axis.length();

        let col_z = (state.transform.translation - hmd.translation).normalize();
        let col_y = up_dir;
        let col_x = col_y.cross(col_z);
        let col_y = col_z.cross(col_x).normalize();
        let col_x = col_x.normalize();

        let rot = Mat3A::from_quat(Quat::from_axis_angle(Vec3::Y, PI));
        state.transform.matrix3 = Mat3A::from_cols(col_x, col_y, col_z).mul_scalar(scale) * rot;

        self.dirty = true;
    }
}

// Contains the window state for a given set
#[derive(Clone, Serialize, Deserialize)]
pub struct OverlayWindowState {
    pub transform: Affine3A,
    pub alpha: f32,
    pub grabbable: bool,
    pub interactable: bool,
    pub positioning: Positioning,
    pub curvature: Option<f32>,
}

impl Default for OverlayWindowState {
    fn default() -> Self {
        Self {
            grabbable: false,
            interactable: false,
            alpha: 1.0,
            positioning: Positioning::Floating,
            curvature: None,
            transform: Affine3A::IDENTITY,
        }
    }
}
