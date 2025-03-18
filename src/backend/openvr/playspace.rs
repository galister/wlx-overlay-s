use glam::{Affine3A, Quat, Vec3A};
use ovr_overlay::{
    chaperone_setup::ChaperoneSetupManager,
    compositor::CompositorManager,
    sys::{EChaperoneConfigFile, ETrackingUniverseOrigin, HmdMatrix34_t},
};

use crate::{
    backend::{common::OverlayContainer, input::InputState},
    state::AppState,
};
use super::{helpers::Affine3AConvert, overlay::OpenVrOverlayData};

struct MoverData<T> {
    pose: Affine3A,
    hand: usize,
    hand_pose: T,
}

pub(super) struct PlayspaceMover {
    universe: ETrackingUniverseOrigin,
    drag: Option<MoverData<Vec3A>>,
    rotate: Option<MoverData<Quat>>,
}

impl PlayspaceMover {
    pub fn new() -> Self {
        Self {
            universe: ETrackingUniverseOrigin::TrackingUniverseRawAndUncalibrated,
            drag: None,
            rotate: None,
        }
    }

    pub fn update(
        &mut self,
        chaperone_mgr: &mut ChaperoneSetupManager,
        overlays: &mut OverlayContainer<OpenVrOverlayData>,
        state: &AppState,
    ) {
        let universe = self.universe.clone();

        if let Some(data) = self.rotate.as_mut() {
            let pointer = &state.input_state.pointers[data.hand];
            if !pointer.now.space_rotate {
                self.rotate = None;
                log::info!("End space rotate");
                return;
            }

            let new_hand =
                Quat::from_affine3(&(data.pose * state.input_state.pointers[data.hand].raw_pose));

            let dq = new_hand * data.hand_pose.conjugate();
            let rel_y = f32::atan2(
                2.0 * (dq.y * dq.w + dq.x * dq.z),
                (2.0 * (dq.w * dq.w + dq.x * dq.x)) - 1.0,
            );

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
                    overlay.state.offset.translation =
                        overlay_transform.transform_point3a(overlay.state.transform.translation);
                }
            });

            data.pose *= space_transform;
            data.hand_pose = new_hand;

            set_working_copy(&universe, chaperone_mgr, &data.pose);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_rotate {
                    let Some(mat) = get_working_copy(&universe, chaperone_mgr) else {
                        log::warn!("Can't space rotate - failed to get zero pose");
                        return;
                    };
                    let hand_pose = Quat::from_affine3(&(mat * pointer.raw_pose));
                    self.rotate = Some(MoverData {
                        pose: mat,
                        hand: i,
                        hand_pose,
                    });
                    self.drag = None;
                    log::info!("Start space rotate");
                    return;
                }
            }
        }

        if let Some(data) = self.drag.as_mut() {
            let pointer = &state.input_state.pointers[data.hand];
            if !pointer.now.space_drag {
                self.drag = None;
                log::info!("End space drag");
                return;
            }

            let new_hand = data
                .pose
                .transform_point3a(state.input_state.pointers[data.hand].raw_pose.translation);
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
                    overlay.state.offset.translation += overlay_offset;
                }
            });

            data.pose.translation += relative_pos;
            data.hand_pose = new_hand;

            set_working_copy(&universe, chaperone_mgr, &data.pose);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_reset {
                    if !pointer.before.space_reset {
                        log::info!("Space reset!");
                        self.reset_offset(chaperone_mgr, overlays);
                    }
                    return;
                }else if pointer.now.space_drag {
                    let Some(mat) = get_working_copy(&universe, chaperone_mgr) else {
                        log::warn!("Can't space drag - failed to get zero pose");
                        return;
                    };
                    let hand_pos = mat.transform_point3a(pointer.raw_pose.translation);
                    self.drag = Some(MoverData {
                        pose: mat,
                        hand: i,
                        hand_pose: hand_pos,
                    });
                    self.rotate = None;
                    log::info!("Start space drag");
                    return;
                }
            }
        }
    }

    pub fn reset_offset(&mut self, chaperone_mgr: &mut ChaperoneSetupManager, overlays: &mut OverlayContainer<OpenVrOverlayData>) {
        chaperone_mgr.revert_working_copy();
        chaperone_mgr.hide_working_set_preview();
        
        overlays.iter_mut().for_each(|overlay| {
            overlay.state.dirty = true;
            overlay.state.offset = Affine3A::IDENTITY;
        });

        if self.drag.is_some() {
            log::info!("Space drag interrupted by manual reset");
            self.drag = None;
        }
        if self.rotate.is_some() {
            log::info!("Space rotate interrupted by manual reset");
            self.rotate = None;
        }
    }

    pub fn fix_floor(&mut self, chaperone_mgr: &mut ChaperoneSetupManager, input: &InputState) {
        let y1 = input.pointers[0].pose.translation.y;
        let y2 = input.pointers[1].pose.translation.y;
        let Some(mut mat) = get_working_copy(&self.universe, chaperone_mgr) else {
            log::warn!("Can't fix floor - failed to get zero pose");
            return;
        };
        let offset = y1.min(y2) - 0.03;
        mat.translation.y += offset;

        chaperone_mgr.revert_working_copy();
        set_working_copy(&self.universe, chaperone_mgr, &mat);
        chaperone_mgr.commit_working_copy(EChaperoneConfigFile::EChaperoneConfigFile_Live);

        if self.drag.is_some() {
            log::info!("Space drag interrupted by fix floor");
            self.drag = None;
        }
        if self.rotate.is_some() {
            log::info!("Space rotate interrupted by fix floor");
            self.rotate = None;
        }
    }

    pub fn playspace_changed(
        &mut self,
        compositor_mgr: &mut CompositorManager,
        _chaperone_mgr: &mut ChaperoneSetupManager,
    ) {
        let new_universe = compositor_mgr.get_tracking_space();
        if new_universe != self.universe {
            log::info!(
                "Playspace changed: {} -> {}",
                universe_str(&self.universe),
                universe_str(&new_universe)
            );
            self.universe = new_universe;
        }

        if self.drag.is_some() {
            log::info!("Space drag interrupted by external change");
            self.drag = None;
        }
        if self.rotate.is_some() {
            log::info!("Space rotate interrupted by external change");
            self.rotate = None;
        }
    }

    pub fn get_universe(&self) -> ETrackingUniverseOrigin {
        self.universe.clone()
    }
}

fn universe_str(universe: &ETrackingUniverseOrigin) -> &'static str {
    match universe {
        ETrackingUniverseOrigin::TrackingUniverseSeated => "Seated",
        ETrackingUniverseOrigin::TrackingUniverseStanding => "Standing",
        ETrackingUniverseOrigin::TrackingUniverseRawAndUncalibrated => "Raw",
    }
}

fn get_working_copy(
    universe: &ETrackingUniverseOrigin,
    chaperone_mgr: &mut ChaperoneSetupManager,
) -> Option<Affine3A> {
    let mat = match universe {
        ETrackingUniverseOrigin::TrackingUniverseStanding => {
            chaperone_mgr.get_working_standing_zero_pose_to_raw_tracking_pose()
        }
        _ => chaperone_mgr.get_working_seated_zero_pose_to_raw_tracking_pose(),
    };
    mat.map(|m| m.to_affine())
}

fn set_working_copy(
    universe: &ETrackingUniverseOrigin,
    chaperone_mgr: &mut ChaperoneSetupManager,
    mat: &Affine3A,
) {
    let mat = HmdMatrix34_t::from_affine(mat);
    match universe {
        ETrackingUniverseOrigin::TrackingUniverseStanding => {
            chaperone_mgr.set_working_standing_zero_pose_to_raw_tracking_pose(&mat)
        }
        _ => chaperone_mgr.set_working_seated_zero_pose_to_raw_tracking_pose(&mat),
    };
    chaperone_mgr.show_working_set_preview();
}
