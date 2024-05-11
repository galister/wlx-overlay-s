use std::ffi::c_void;

use glam::Vec3A;
use libloading::{Library, Symbol};

use crate::{
    backend::common::OverlayContainer,
    state::AppState,
};

use super::{helpers, overlay::OpenXrOverlayData};

pub(super) struct PlayspaceMover {
    drag_hand: Option<usize>,
    offset: Vec3A,
    start_position: Vec3A,

    has_pressed: bool,
    has_unpressed: bool,
    pressed_timer: std::time::Instant,

    libmonado: Library,
    mnd_root: *mut c_void,
    playspace_move: extern "C" fn(*mut c_void, f32, f32, f32) -> i32,
}

impl PlayspaceMover {
    pub fn new() -> Self {
        unsafe {
            let libmonado = helpers::find_libmonado().unwrap_or_else(|e| {
                log::error!("Failed to find libmonado: {}", e);
                std::process::exit(1);
            });

            let root_create: Symbol<extern "C" fn(*mut *mut c_void) -> i32> =
                libmonado.get(b"mnd_root_create").unwrap();
            let playspace_move: Symbol<extern "C" fn(*mut c_void, f32, f32, f32) -> i32> =
                libmonado.get(b"mnd_root_playspace_move").unwrap();
            let playspace_move_raw = *playspace_move;

            let mut root: *mut c_void = std::ptr::null_mut();

            let ret = root_create(&mut root);

            if ret != 0 {
                log::error!("Failed to create root, error code: {}", ret);
            }

            Self {
                drag_hand: None,
                offset: Vec3A::ZERO,
                start_position: Vec3A::ZERO,

                has_pressed: false,
                has_unpressed: false,
                pressed_timer: std::time::Instant::now(),

                libmonado,
                mnd_root: root,
                playspace_move: playspace_move_raw,
            }
        }
    }

    pub fn update(&mut self, overlays: &mut OverlayContainer<OpenXrOverlayData>, state: &AppState) {
        if self.has_unpressed {
            if self.pressed_timer.elapsed().as_secs_f32() > 0.2 {
                self.has_unpressed = false;
                self.has_pressed = false;
            }
        }

        if let Some(hand) = self.drag_hand {
            let pointer = &state.input_state.pointers[hand];
            if !pointer.now.space_drag {
                self.drag_hand = None;
                log::info!("End space drag");
                return;
            }

            let hand_pos = state.input_state.pointers[hand].pose.translation;
            let relative_pos = hand_pos - self.start_position;

            overlays.iter_mut().for_each(|overlay| {
                if overlay.state.grabbable {
                    overlay.state.dirty = true;
                    overlay.state.transform.translation += relative_pos * -1.0;
                }
            });

            self.offset += relative_pos;
            self.apply_offset();
        } else {
            let mut pressed = false;
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_drag {
                    pressed = true;
                    if !self.has_pressed {
                        self.has_pressed = true;
                        break;
                    }

                    if self.has_pressed && self.has_unpressed {
                        self.drag_hand = Some(i);
                        self.start_position = pointer.pose.translation;
                        self.has_pressed = false;
                        self.has_unpressed = false;
                        break;
                    }
                }
            }

            if !pressed && self.has_pressed && !self.has_unpressed {
                self.has_unpressed = true;
                self.pressed_timer = std::time::Instant::now();
            }
        }
    }

    pub fn reset(&mut self) {
        self.offset = Vec3A::ZERO;
        self.start_position = Vec3A::ZERO;
    }

    fn apply_offset(&mut self) {
        (self.playspace_move)(self.mnd_root, self.offset.x, self.offset.y, self.offset.z);
    }
}
