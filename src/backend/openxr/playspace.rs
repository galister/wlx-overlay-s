use std::ffi::c_void;

use glam::Vec3A;
use libloading::{Library, Symbol};

use crate::{backend::common::OverlayContainer, state::AppState};

use super::{helpers, overlay::OpenXrOverlayData};

pub(super) struct PlayspaceMover {
    drag_hand: Option<usize>,
    offset: Vec3A,
    start_position: Vec3A,

    libmonado: Library,
    mnd_root: *mut c_void,
    playspace_move: extern "C" fn(*mut c_void, f32, f32, f32) -> i32,
}

impl PlayspaceMover {
    pub fn try_new() -> anyhow::Result<Self> {
        unsafe {
            let libmonado = helpers::find_libmonado()?;

            let root_create: Symbol<extern "C" fn(*mut *mut c_void) -> i32> =
                libmonado.get(b"mnd_root_create\0")?;
            let playspace_move: Symbol<extern "C" fn(*mut c_void, f32, f32, f32) -> i32> =
                libmonado.get(b"mnd_root_playspace_move\0")?;
            let playspace_move_raw = *playspace_move;

            let mut root: *mut c_void = std::ptr::null_mut();

            let ret = root_create(&mut root);

            if ret != 0 {
                anyhow::bail!("Failed to create root, code: {}", ret);
            }

            Ok(Self {
                drag_hand: None,
                offset: Vec3A::ZERO,
                start_position: Vec3A::ZERO,

                libmonado,
                mnd_root: root,
                playspace_move: playspace_move_raw,
            })
        }
    }

    pub fn update(&mut self, overlays: &mut OverlayContainer<OpenXrOverlayData>, state: &AppState) {
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
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_drag && !pointer.before.space_drag {
                    log::info!("Start space drag");
                    self.drag_hand = Some(i);
                    self.start_position = pointer.pose.translation;
                    break;
                }
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
