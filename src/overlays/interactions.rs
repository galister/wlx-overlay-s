use std::{collections::VecDeque, time::Instant};

use glam::{Affine3A, Vec2, Vec3};

use crate::state::AppState;

pub const HAND_LEFT: usize = 0;
pub const HAND_RIGHT: usize = 1;

pub const POINTER_NORM: u16 = 0;
pub const POINTER_SHIFT: u16 = 1;
pub const POINTER_ALT: u16 = 2;

pub trait InteractionHandler {
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit);
    fn on_left(&mut self, app: &mut AppState, hand: usize);
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool);
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32);
}

// --- Dummies & plumbing below ---

impl Default for PointerState {
    fn default() -> Self {
        Self {
            click: false,
            grab: false,
            show_hide: false,
            scroll: 0.,
        }
    }
}

pub struct DummyInteractionHandler;

impl InteractionHandler for DummyInteractionHandler {
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}
    fn on_hover(&mut self, _app: &mut AppState, _hit: &PointerHit) {}
    fn on_pointer(&mut self, _app: &mut AppState, _hit: &PointerHit, _pressed: bool) {}
    fn on_scroll(&mut self, _app: &mut AppState, _hit: &PointerHit, _delta: f32) {}
}
