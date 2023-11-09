use std::{collections::VecDeque, time::Instant};

use glam::{Affine3A, Vec2, Vec3, Vec3A};
use ovr_overlay::TrackedDeviceIndex;

pub struct InputState<TState, THand> {
    pub hmd: Affine3A,
    pub pointers: [Pointer<THand>; 2],
    pub devices: Vec<TrackedDevice>,
    pub(super) data: TState,
}

impl<TState, THand> InputState<TState, THand> {
    pub fn pre_update(&mut self) {
        self.pointers[0].before = self.pointers[0].now;
        self.pointers[1].before = self.pointers[1].now;
    }

    pub fn post_update(&mut self) {
        for hand in &mut self.pointers {
            if hand.now.click_modifier_right {
                hand.interaction.mode = PointerMode::Right;
                continue;
            }

            if hand.now.click_modifier_middle {
                hand.interaction.mode = PointerMode::Middle;
                continue;
            }

            let hmd_up = self.hmd.transform_vector3a(Vec3A::Y);
            let dot =
                hmd_up.dot(hand.pose.transform_vector3a(Vec3A::X)) * (1.0 - 2.0 * hand.hand as f32);

            hand.interaction.mode = if dot < -0.85 {
                PointerMode::Right
            } else if dot > 0.7 {
                PointerMode::Middle
            } else {
                PointerMode::Left
            };

            let middle_click_orientation = false;
            let right_click_orientation = false;
            match hand.interaction.mode {
                PointerMode::Middle => {
                    if !middle_click_orientation {
                        hand.interaction.mode = PointerMode::Left;
                    }
                }
                PointerMode::Right => {
                    if !right_click_orientation {
                        hand.interaction.mode = PointerMode::Left;
                    }
                }
                _ => {}
            };
        }
    }
}

pub struct Pointer<THand> {
    pub hand: usize,
    pub pose: Affine3A,
    pub now: PointerState,
    pub before: PointerState,
    pub(super) interaction: InteractionState,
    pub(super) data: THand,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PointerState {
    pub scroll: f32,
    pub click: bool,
    pub grab: bool,
    pub alt_click: bool,
    pub show_hide: bool,
    pub space_drag: bool,
    pub click_modifier_right: bool,
    pub click_modifier_middle: bool,
}

pub struct InteractionState {
    pub mode: PointerMode,
    pub grabbed: Option<GrabData>,
    pub clicked_id: Option<usize>,
    pub hovered_id: Option<usize>,
    pub release_actions: VecDeque<Box<dyn Fn()>>,
    pub next_push: Instant,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            mode: PointerMode::Left,
            grabbed: None,
            clicked_id: None,
            hovered_id: None,
            release_actions: VecDeque::new(),
            next_push: Instant::now(),
        }
    }
}

pub struct PointerHit {
    pub hand: usize,
    pub mode: PointerMode,
    pub primary: bool,
    pub uv: Vec2,
    pub dist: f32,
}

struct RayHit {
    idx: usize,
    ray_pos: Vec3,
    hit_pos: Vec3,
    uv: Vec2,
    dist: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GrabData {
    pub offset: Vec3,
    pub grabbed_id: usize,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default)]
pub enum PointerMode {
    #[default]
    Left,
    Right,
    Middle,
}

pub struct TrackedDevice {
    pub index: TrackedDeviceIndex,
    pub valid: bool,
    pub soc: Option<f32>,
    pub charging: bool,
    pub role: TrackedDeviceRole,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedDeviceRole {
    None,
    Hmd,
    LeftHand,
    RightHand,
    Tracker,
}
