use std::{collections::VecDeque, time::Instant};

use glam::{Affine3A, Vec2, Vec3A};

#[cfg(feature = "openvr")]
use ovr_overlay::TrackedDeviceIndex;
use smallvec::{smallvec, SmallVec};

use crate::state::AppState;

use super::{
    common::{raycast, OverlayContainer},
    overlay::OverlayData,
};

pub struct TrackedDevice {
    #[cfg(feature = "openvr")]
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

pub struct InputState {
    pub hmd: Affine3A,
    pub pointers: [Pointer; 2],
    pub devices: Vec<TrackedDevice>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            hmd: Affine3A::IDENTITY,
            pointers: [Pointer::new(0), Pointer::new(1)],
            devices: Vec::new(),
        }
    }

    pub fn pre_update(&mut self) {
        self.pointers[0].before = self.pointers[0].now;
        self.pointers[1].before = self.pointers[1].now;
    }

    pub fn post_update(&mut self) {
        for hand in &mut self.pointers {
            #[cfg(debug_assertions)]
            {
                if hand.now.click != hand.before.click {
                    log::debug!("Hand {}: click {}", hand.idx, hand.now.click);
                }
                if hand.now.grab != hand.before.grab {
                    log::debug!("Hand {}: grab {}", hand.idx, hand.now.grab);
                }
                if hand.now.alt_click != hand.before.alt_click {
                    log::debug!("Hand {}: alt_click {}", hand.idx, hand.now.alt_click);
                }
                if hand.now.show_hide != hand.before.show_hide {
                    log::debug!("Hand {}: show_hide {}", hand.idx, hand.now.show_hide);
                }
                if hand.now.space_drag != hand.before.space_drag {
                    log::debug!("Hand {}: space_drag {}", hand.idx, hand.now.space_drag);
                }
                if hand.now.click_modifier_right != hand.before.click_modifier_right {
                    log::debug!(
                        "Hand {}: click_modifier_right {}",
                        hand.idx,
                        hand.now.click_modifier_right
                    );
                }
                if hand.now.click_modifier_middle != hand.before.click_modifier_middle {
                    log::debug!(
                        "Hand {}: click_modifier_middle {}",
                        hand.idx,
                        hand.now.click_modifier_middle
                    );
                }
            }

            if hand.now.click {
                hand.last_click = Instant::now();
            }

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
                hmd_up.dot(hand.pose.transform_vector3a(Vec3A::X)) * (1.0 - 2.0 * hand.idx as f32);

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
    pub haptics: Option<f32>,
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
            haptics: None,
        }
    }
}

pub struct Pointer {
    pub idx: usize,
    pub pose: Affine3A,
    pub raw_pose: Affine3A,
    pub now: PointerState,
    pub before: PointerState,
    pub last_click: Instant,
    pub(super) interaction: InteractionState,
}

impl Pointer {
    pub fn new(idx: usize) -> Self {
        debug_assert!(idx == 0 || idx == 1);
        Self {
            idx,
            pose: Affine3A::IDENTITY,
            raw_pose: Affine3A::IDENTITY,
            now: Default::default(),
            before: Default::default(),
            last_click: Instant::now(),
            interaction: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Default)]
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

pub struct Haptics {
    pub intensity: f32,
    pub duration: f32,
    pub frequency: f32,
}

pub trait InteractionHandler {
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics>;
    fn on_left(&mut self, app: &mut AppState, pointer: usize);
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool);
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32);
}

pub struct DummyInteractionHandler;

impl InteractionHandler for DummyInteractionHandler {
    fn on_left(&mut self, _app: &mut AppState, _pointer: usize) {}
    fn on_hover(&mut self, _app: &mut AppState, _hit: &PointerHit) -> Option<Haptics> {
        None
    }
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
    Special,
}

pub fn interact<O>(
    overlays: &mut OverlayContainer<O>,
    app: &mut AppState,
) -> [(f32, Option<Haptics>); 2]
where
    O: Default,
{
    if app.input_state.pointers[1].last_click > app.input_state.pointers[0].last_click {
        let right = interact_hand(1, overlays, app);
        let left = interact_hand(0, overlays, app);
        [left, right]
    } else {
        let left = interact_hand(0, overlays, app);
        let right = interact_hand(1, overlays, app);
        [left, right]
    }
}

fn interact_hand<O>(
    idx: usize,
    overlays: &mut OverlayContainer<O>,
    app: &mut AppState,
) -> (f32, Option<Haptics>)
where
    O: Default,
{
    let hmd = &app.input_state.hmd;
    let mut pointer = &mut app.input_state.pointers[idx];
    if let Some(grab_data) = pointer.interaction.grabbed {
        if let Some(grabbed) = overlays.mut_by_id(grab_data.grabbed_id) {
            pointer.handle_grabbed(grabbed, hmd);
        } else {
            log::warn!("Grabbed overlay {} does not exist", grab_data.grabbed_id);
            pointer.interaction.grabbed = None;
        }
        return (0.1, None);
    }

    let Some(mut hit) = pointer.get_nearest_hit(overlays) else {
        if let Some(hovered_id) = pointer.interaction.hovered_id.take() {
            if let Some(hovered) = overlays.mut_by_id(hovered_id) {
                hovered.backend.on_left(app, idx);
            }
            pointer = &mut app.input_state.pointers[idx];
            pointer.interaction.hovered_id = None;
        }
        if let Some(clicked_id) = pointer.interaction.clicked_id.take() {
            if let Some(clicked) = overlays.mut_by_id(clicked_id) {
                let hit = PointerHit {
                    pointer: pointer.idx,
                    overlay: clicked_id,
                    mode: pointer.interaction.mode,
                    ..Default::default()
                };
                clicked.backend.on_pointer(app, &hit, false);
            }
        }
        return (0.0, None); // no hit
    };

    if let Some(hovered_id) = pointer.interaction.hovered_id {
        if hovered_id != hit.overlay {
            if let Some(old_hovered) = overlays.mut_by_id(hovered_id) {
                if Some(pointer.idx) == old_hovered.primary_pointer {
                    old_hovered.primary_pointer = None;
                }
                old_hovered.backend.on_left(app, idx);
                pointer = &mut app.input_state.pointers[idx];
            }
        }
    }
    let Some(hovered) = overlays.mut_by_id(hit.overlay) else {
        log::warn!("Hit overlay {} does not exist", hit.overlay);
        return (0.0, None); // no hit
    };

    pointer.interaction.hovered_id = Some(hit.overlay);

    if let Some(primary_pointer) = hovered.primary_pointer {
        if hit.pointer <= primary_pointer {
            hovered.primary_pointer = Some(hit.pointer);
            hit.primary = true;
        }
    } else {
        hovered.primary_pointer = Some(hit.pointer);
        hit.primary = true;
    }

    #[cfg(debug_assertions)]
    log::trace!("Hit: {} {:?}", hovered.state.name, hit);

    if pointer.now.grab && !pointer.before.grab && hovered.state.grabbable {
        pointer.start_grab(hovered);
        return (
            hit.dist,
            Some(Haptics {
                intensity: 0.25,
                duration: 0.1,
                frequency: 0.1,
            }),
        );
    }

    let haptics = hovered.backend.on_hover(app, &hit);
    pointer = &mut app.input_state.pointers[idx];

    if pointer.now.scroll.abs() > 0.1 {
        let scroll = pointer.now.scroll;
        hovered.backend.on_scroll(app, &hit, scroll);
        pointer = &mut app.input_state.pointers[idx];
    }

    if pointer.now.click && !pointer.before.click {
        pointer.interaction.clicked_id = Some(hit.overlay);
        hovered.backend.on_pointer(app, &hit, true);
    } else if !pointer.now.click && pointer.before.click {
        if let Some(clicked_id) = pointer.interaction.clicked_id.take() {
            if let Some(clicked) = overlays.mut_by_id(clicked_id) {
                clicked.backend.on_pointer(app, &hit, false);
            }
        } else {
            hovered.backend.on_pointer(app, &hit, false);
        }
    }
    (hit.dist, haptics)
}

impl Pointer {
    fn get_nearest_hit<O>(&mut self, overlays: &mut OverlayContainer<O>) -> Option<PointerHit>
    where
        O: Default,
    {
        let mut hits: SmallVec<[RayHit; 8]> = smallvec!();

        for overlay in overlays.iter() {
            if !overlay.state.want_visible || !overlay.state.interactable {
                continue;
            }

            if let Some(hit) = self.ray_test(overlay.state.id, &overlay.state.transform) {
                if hit.dist.is_infinite() || hit.dist.is_nan() {
                    continue;
                }
                hits.push(hit);
            }
        }

        hits.sort_by(|a, b| a.dist.total_cmp(&b.dist));

        for hit in hits.iter() {
            let overlay = overlays.get_by_id(hit.overlay).unwrap(); // safe because we just got the id from the overlay

            let uv = overlay
                .state
                .transform
                .inverse()
                .transform_point3a(hit.hit_pos)
                .truncate();

            let uv = overlay.state.interaction_transform.transform_point2(uv);

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
        log::info!("Hand {}: grabbed {}", self.idx, overlay.state.name);
    }

    fn handle_grabbed<O>(&mut self, overlay: &mut OverlayData<O>, hmd: &Affine3A)
    where
        O: Default,
    {
        if self.now.grab {
            if let Some(grab_data) = self.interaction.grabbed.as_mut() {
                if self.now.click {
                    self.interaction.mode = PointerMode::Special;
                    let cur_scale = overlay.state.transform.x_axis.length();
                    if cur_scale < 0.1 && self.now.scroll > 0.0 {
                        return;
                    }
                    if cur_scale > 20. && self.now.scroll < 0.0 {
                        return;
                    }

                    overlay.state.transform.matrix3 = overlay
                        .state
                        .transform
                        .matrix3
                        .mul_scalar(1.0 - 0.025 * self.now.scroll);
                } else {
                    grab_data.offset.z -= self.now.scroll * 0.05;
                }
                overlay.state.transform.translation = self.pose.transform_point3a(grab_data.offset);
                overlay.state.realign(hmd);
                overlay.state.dirty = true;
            } else {
                log::error!("Grabbed overlay {} does not exist", overlay.state.id);
                self.interaction.grabbed = None;
            }
        } else {
            overlay.state.saved_point = Some(
                hmd.inverse()
                    .transform_point3a(overlay.state.transform.translation),
            );
            self.interaction.grabbed = None;
            log::info!("Hand {}: dropped {}", self.idx, overlay.state.name);
        }
    }

    fn ray_test(&self, overlay: usize, plane: &Affine3A) -> Option<RayHit> {
        let Some((hit_pos, dist)) = raycast(&self.pose, Vec3A::NEG_Z, plane, Vec3A::NEG_Z) else {
            return None;
        };

        Some(RayHit {
            overlay,
            hit_pos,
            dist,
        })
    }
}
