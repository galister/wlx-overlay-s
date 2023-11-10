use std::{collections::VecDeque, time::Instant};

use glam::{Affine3A, Vec2, Vec3A};
use log::warn;
use ovr_overlay::TrackedDeviceIndex;
use tinyvec::array_vec;

use crate::state::AppState;

use super::{common::OverlayContainer, overlay::OverlayData};

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

pub struct Pointer<THand> {
    pub idx: usize,
    pub hand: u8,
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

#[derive(Debug, Clone, Copy, Default)]
pub struct PointerHit {
    pub pointer: usize,
    pub overlay: usize,
    pub mode: PointerMode,
    pub primary: bool,
    pub uv: Vec2,
    pub dist: f32,
}

pub trait InteractionHandler {
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit);
    fn on_left(&mut self, app: &mut AppState, pointer: usize);
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool);
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32);
}

pub struct DummyInteractionHandler;

impl InteractionHandler for DummyInteractionHandler {
    fn on_left(&mut self, _app: &mut AppState, _pointer: usize) {}
    fn on_hover(&mut self, _app: &mut AppState, _hit: &PointerHit) {}
    fn on_pointer(&mut self, _app: &mut AppState, _hit: &PointerHit, _pressed: bool) {}
    fn on_scroll(&mut self, _app: &mut AppState, _hit: &PointerHit, _delta: f32) {}
}

#[derive(Debug, Clone, Copy, Default)]
struct RayHit {
    overlay: usize,
    hit_pos: Vec3A,
    dist: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GrabData {
    pub offset: Vec3A,
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

impl<THand> Pointer<THand> {
    pub fn interact<O>(&mut self, overlays: &mut OverlayContainer<O>, app: &mut AppState)
    where
        O: Default,
    {
        if let Some(grab_data) = self.interaction.grabbed {
            if let Some(grabbed) = overlays.mut_by_id(grab_data.grabbed_id) {
                self.handle_grabbed(grabbed, grab_data.offset);
            } else {
                warn!("Grabbed overlay {} does not exist", grab_data.grabbed_id);
                self.interaction.grabbed = None;
            }
            return;
        }

        let Some(mut hit) = self.get_nearest_hit(overlays) else {
            if let Some(hovered_id) = self.interaction.hovered_id.take() {
                if let Some(hovered) = overlays.mut_by_id(hovered_id) {
                    hovered.backend.on_left(app, self.idx);
                }
                self.interaction.hovered_id = None;
            }
            if let Some(clicked_id) = self.interaction.clicked_id.take() {
                if let Some(clicked) = overlays.mut_by_id(clicked_id) {
                    let hit = PointerHit {
                        pointer: self.idx,
                        overlay: clicked_id,
                        mode: self.interaction.mode,
                        ..Default::default()
                    };
                    clicked.backend.on_pointer(app, &hit, false);
                }
            }
            return;
        };

        if let Some(hovered_id) = self.interaction.hovered_id {
            if hovered_id != hit.overlay {
                if let Some(old_hovered) = overlays.mut_by_id(hovered_id) {
                    if Some(self.idx) == old_hovered.primary_pointer {
                        old_hovered.primary_pointer = None;
                    }
                    old_hovered.backend.on_left(app, self.idx);
                }
            }
        }
        let Some(hovered) = overlays.mut_by_id(hit.overlay) else {
            warn!("Hit overlay {} does not exist", hit.overlay);
            return;
        };

        self.interaction.hovered_id = Some(hit.overlay);

        if let Some(primary_pointer) = hovered.primary_pointer {
            if hit.pointer < primary_pointer {
                hovered.primary_pointer = Some(hit.pointer);
                hit.primary = true;
            }
        } else {
            hovered.primary_pointer = Some(hit.pointer);
            hit.primary = true;
        }
        hovered.backend.on_hover(app, &hit);

        if self.now.scroll.abs() > 0.1 {
            hovered.backend.on_scroll(app, &hit, self.now.scroll);
        }

        if self.now.click && !self.before.click {
            self.interaction.clicked_id = Some(hit.overlay);
            hovered.backend.on_pointer(app, &hit, true);
        } else if !self.now.click && self.before.click {
            if let Some(clicked_id) = self.interaction.clicked_id.take() {
                if let Some(clicked) = overlays.mut_by_id(clicked_id) {
                    clicked.backend.on_pointer(app, &hit, false);
                }
            } else {
                hovered.backend.on_pointer(app, &hit, false);
            }
        }
    }

    fn get_nearest_hit<O>(&mut self, overlays: &mut OverlayContainer<O>) -> Option<PointerHit>
    where
        O: Default,
    {
        let mut hits = array_vec!([RayHit; 8]);

        for overlay in overlays.iter() {
            if !overlay.state.want_visible {
                continue;
            }

            if let Some(hit) = self.ray_test(overlay.state.id, &overlay.state.transform) {
                hits.try_push(hit);
            }
        }

        hits.sort_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap());

        for hit in hits.iter() {
            let uv = overlays
                .get_by_id(hit.overlay)
                .unwrap() // this is safe
                .state
                .transform
                .inverse()
                .transform_point3a(hit.hit_pos)
                .truncate();

            if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
                continue;
            }

            return Some(PointerHit {
                pointer: self.idx,
                overlay: hit.overlay,
                mode: self.interaction.mode,
                primary: false,
                uv,
                dist: hit.dist,
            });
        }

        None
    }

    fn start_grab<O>(&mut self, overlay: &mut OverlayData<O>)
    where
        O: Default,
    {
        let offset = self
            .pose
            .inverse()
            .transform_point3a(overlay.state.transform.translation);

        self.interaction.grabbed = Some(GrabData {
            offset,
            grabbed_id: overlay.state.id,
        });
    }
    fn handle_grabbed<O>(&mut self, overlay: &mut OverlayData<O>, offset: Vec3A)
    where
        O: Default,
    {
        if self.now.grab {
            overlay.state.transform.translation = self.pose.transform_point3a(offset);

            if self.now.click && !self.before.click {
                warn!("todo: click-while-grabbed");
            }

            match self.interaction.mode {
                PointerMode::Left => {
                    overlay.state.transform.translation.y += self.now.scroll * 0.01;
                }
                _ => {
                    overlay.state.transform.matrix3 = overlay
                        .state
                        .transform
                        .matrix3
                        .mul_scalar(1.0 + 0.01 * self.now.scroll);
                }
            }
        } else {
            overlay.state.spawn_point = overlay.state.transform.translation;
            self.interaction.grabbed = None;
        }
    }
    fn ray_test(&self, overlay: usize, plane: &Affine3A) -> Option<RayHit> {
        let plane_normal = plane.transform_vector3a(Vec3A::NEG_Z);
        let ray_dir = self.pose.transform_vector3a(Vec3A::NEG_Z);

        let d = plane.translation.dot(-plane_normal);
        let dist = -(d + self.pose.translation.dot(plane_normal)) / ray_dir.dot(plane_normal);

        let hit_pos = self.pose.translation + ray_dir * dist;

        Some(RayHit {
            overlay,
            hit_pos,
            dist,
        })
    }
}
