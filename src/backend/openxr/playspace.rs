use glam::{Affine3A, Quat, Vec3A};
use libmonado_rs::{Monado, Pose, ReferenceSpaceType};

use crate::{
    backend::{common::OverlayContainer, input::InputState},
    state::AppState,
};

use super::overlay::OpenXrOverlayData;

#[repr(C)]
#[derive(Default, Debug)]
struct XrtPose {
    orientation: [f32; 4],
    position: [f32; 3],
}

struct MoverData<T> {
    pose: Affine3A,
    hand: usize,
    hand_pose: T,
}

pub(super) struct PlayspaceMover {
    last_transform: Affine3A,
    drag: Option<MoverData<Vec3A>>,
    rotate: Option<MoverData<Quat>>,
}

impl PlayspaceMover {
    pub fn new(monado: &mut Monado) -> anyhow::Result<Self> {
        log::info!("Monado: using space offset API");

        let Ok(stage) = monado.get_reference_space_offset(ReferenceSpaceType::Stage) else {
            anyhow::bail!("Space offsets not supported.");
        };

        log::debug!("STAGE is at {:?}, {:?}", stage.position, stage.orientation);

        // initial offset
        let last_transform =
            Affine3A::from_rotation_translation(stage.orientation.into(), stage.position.into());

        Ok(Self {
            last_transform,

            drag: None,
            rotate: None,
        })
    }

    pub fn update(
        &mut self,
        overlays: &mut OverlayContainer<OpenXrOverlayData>,
        state: &AppState,
        monado: &mut Monado,
    ) {
        for (_i, pointer) in state.input_state.pointers.iter().enumerate() {
            if pointer.now.space_reset {
                if !pointer.before.space_reset {
                    log::info!("Space reset");
                    self.reset_offset(monado);
                }
                return;
            }
        }

        if let Some(mut data) = self.rotate.take() {
            let pointer = &state.input_state.pointers[data.hand];
            if !pointer.now.space_rotate {
                self.last_transform = data.pose;
                log::info!("End space rotate");
                return;
            }

            let new_hand =
                Quat::from_affine3(&(data.pose * state.input_state.pointers[data.hand].pose));

            let dq = new_hand * data.hand_pose.conjugate();
            let rel_y = f32::atan2(
                2.0 * (dq.y * dq.w + dq.x * dq.z),
                (2.0 * (dq.w * dq.w + dq.x * dq.x)) - 1.0,
            );

            //let mut space_transform = Affine3A::from_rotation_translation(dq, Vec3::ZERO);
            let mut space_transform = Affine3A::from_rotation_y(rel_y);
            let offset = (space_transform.transform_vector3a(state.input_state.hmd.translation)
                - state.input_state.hmd.translation)
                * -1.0;
            let mut overlay_transform = Affine3A::from_rotation_y(-rel_y);

            overlay_transform.translation = offset;
            space_transform.translation = offset;

            overlays.iter_mut().for_each(|overlay| {
                if overlay.state.grabbable {
                    overlay.state.dirty = true;
                    overlay.state.transform.translation =
                        overlay_transform.transform_point3a(overlay.state.transform.translation);
                }
            });

            data.pose *= space_transform;
            data.hand_pose = new_hand;

            self.apply_offset(data.pose, monado);
            self.rotate = Some(data);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_rotate {
                    let hand_pose = Quat::from_affine3(&self.last_transform);
                    self.rotate = Some(MoverData {
                        pose: self.last_transform,
                        hand: i,
                        hand_pose,
                    });
                    self.drag = None;
                    log::info!("Start space rotate");
                    return;
                }
            }
        }

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
            let relative_pos =
                (new_hand - data.hand_pose) * state.session.config.space_drag_multiplier;

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

            self.apply_offset(data.pose, monado);
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

    pub fn reset_offset(&mut self, monado: &mut Monado) {
        if self.drag.is_some() {
            log::info!("Space drag interrupted by manual reset");
            self.drag = None;
        }
        if self.rotate.is_some() {
            log::info!("Space rotate interrupted by manual reset");
            self.rotate = None;
        }

        self.last_transform = Affine3A::IDENTITY;
        self.apply_offset(self.last_transform, monado);
    }

    pub fn fix_floor(&mut self, input: &InputState, monado: &mut Monado) {
        if self.drag.is_some() {
            log::info!("Space drag interrupted by fix floor");
            self.drag = None;
        }
        if self.rotate.is_some() {
            log::info!("Space rotate interrupted by fix floor");
            self.rotate = None;
        }

        let y1 = input.pointers[0].pose.translation.y;
        let y2 = input.pointers[1].pose.translation.y;
        let delta = y1.min(y2) - 0.03;
        self.last_transform.translation.y += delta;
        self.apply_offset(self.last_transform, monado);
    }

    fn apply_offset(&self, transform: Affine3A, monado: &mut Monado) {
        let pose = Pose {
            position: transform.translation.into(),
            orientation: Quat::from_affine3(&transform).into(),
        };
        let _ = monado.set_reference_space_offset(ReferenceSpaceType::Stage, pose);
    }
}
