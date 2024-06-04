use std::ffi::c_void;

use glam::{Affine3A, Quat, Vec3A};
use libloading::{Library, Symbol};

use crate::{
    backend::{common::OverlayContainer, input::InputState},
    state::AppState,
};

use super::{helpers, overlay::OpenXrOverlayData};

#[repr(C)]
struct XrtPose {
    orientation: [f32; 4],
    position: [f32; 3],
}

struct MoverData<T> {
    pose: Affine3A,
    hand: usize,
    hand_pose: T,
}

// Legacy implementation
type PlaySpaceMove = extern "C" fn(*mut c_void, f32, f32, f32) -> i32;

// New implementation
type ApplyStageOffset = extern "C" fn(*mut c_void, *const XrtPose) -> i32;

enum ApiImpl {
    None,
    PlaySpaceMove(PlaySpaceMove),
    ApplyStageOffset(ApplyStageOffset),
}

pub(super) struct PlayspaceMover {
    last_transform: Affine3A,
    drag: Option<MoverData<Vec3A>>,

    libmonado: Library,
    mnd_root: *mut c_void,
    api_impl: ApiImpl,
}

impl PlayspaceMover {
    pub fn try_new() -> anyhow::Result<Self> {
        unsafe {
            let libmonado = helpers::find_libmonado()?;

            let root_create: Symbol<extern "C" fn(*mut *mut c_void) -> i32> =
                libmonado.get(b"mnd_root_create\0")?;

            let mut api_impl = ApiImpl::None;
            if let Ok(playspace_move) = libmonado.get(b"mnd_root_playspace_move\0") {
                log::info!("Monado: using playspace_move");
                api_impl = ApiImpl::PlaySpaceMove(*playspace_move);
            } else if let Ok(apply_stage_offset) = libmonado.get(b"mnd_root_apply_stage_offset\0") {
                log::info!("Monado: using apply_stage_offset");
                api_impl = ApiImpl::ApplyStageOffset(*apply_stage_offset);
            }

            if let ApiImpl::None = api_impl {
                anyhow::bail!("Monado does not support playspace mover.");
            }

            let mut root: *mut c_void = std::ptr::null_mut();

            let ret = root_create(&mut root);

            if ret != 0 {
                anyhow::bail!("Failed to create root, code: {}", ret);
            }

            Ok(Self {
                last_transform: Affine3A::IDENTITY,
                drag: None,

                libmonado,
                mnd_root: root,
                api_impl,
            })
        }
    }

    pub fn update(&mut self, overlays: &mut OverlayContainer<OpenXrOverlayData>, state: &AppState) {
        if let Some(mut data) = self.drag.take() {
            let pointer = &state.input_state.pointers[data.hand];
            if !pointer.now.space_drag {
                self.last_transform = data.pose;
                log::info!("End space drag");
                return;
            }

            let new_hand = data
                .pose
                .transform_point3a(state.input_state.pointers[data.hand].pose.translation);
            let relative_pos = new_hand - data.hand_pose;

            if relative_pos.length_squared() > 1000.0 {
                log::warn!("Space drag too fast, ignoring");
                return;
            }

            let overlay_offset = data.pose.inverse().transform_vector3a(relative_pos) * -1.0;

            overlays.iter_mut().for_each(|overlay| {
                if overlay.state.grabbable {
                    overlay.state.dirty = true;
                    overlay.state.transform.translation += overlay_offset;
                }
            });

            data.pose.translation += relative_pos;
            data.hand_pose = new_hand;

            self.apply_offset(data.pose);
            self.drag = Some(data);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_drag {
                    let hand_pos = self
                        .last_transform
                        .transform_point3a(pointer.pose.translation);
                    self.drag = Some(MoverData {
                        pose: self.last_transform,
                        hand: i,
                        hand_pose: hand_pos,
                    });
                    log::info!("Start space drag");
                    return;
                }
            }
        }
    }

    pub fn reset_offset(&mut self) {
        if self.drag.is_some() {
            log::info!("Cannot reset offset while dragging.");
            return;
        }

        self.last_transform = Affine3A::IDENTITY;
        self.apply_offset(self.last_transform);
    }

    pub fn fix_floor(&mut self, input: &InputState) {
        if self.drag.is_some() {
            log::info!("Cannot fix floor while dragging.");
            return;
        }

        let y1 = input.pointers[0].pose.translation.y;
        let y2 = input.pointers[1].pose.translation.y;
        let delta = y1.min(y2) - 0.03;
        self.last_transform.translation.y += delta;
        self.apply_offset(self.last_transform);
    }

    fn apply_offset(&self, transform: Affine3A) {
        match self.api_impl {
            ApiImpl::PlaySpaceMove(playspace_move) => {
                (playspace_move)(
                    self.mnd_root,
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                );
            }
            ApiImpl::ApplyStageOffset(apply_stage_offset) => {
                let xrt_pose = XrtPose {
                    orientation: Quat::from_affine3(&transform).into(),
                    position: transform.translation.into(),
                };
                (apply_stage_offset)(self.mnd_root, &xrt_pose);
            }
            ApiImpl::None => {}
        }
    }
}
