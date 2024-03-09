use glam::{Affine3A, Vec3A};
use ovr_overlay::{
    chaperone_setup::ChaperoneSetupManager,
    compositor::CompositorManager,
    sys::{EChaperoneConfigFile, ETrackingUniverseOrigin, HmdMatrix34_t, HmdVector3_t},
};

use crate::{
    backend::{common::OverlayContainer, input::InputState},
    state::AppState,
};

use super::{helpers::Affine3AConvert, overlay::OpenVrOverlayData};

struct DragData {
    pose: Affine3A,
    hand: usize,
    hand_pos: Vec3A,
}

pub(super) struct PlayspaceMover {
    universe: ETrackingUniverseOrigin,
    last: Option<DragData>,
}

impl PlayspaceMover {
    pub fn new() -> Self {
        Self {
            universe: ETrackingUniverseOrigin::TrackingUniverseRawAndUncalibrated,
            last: None,
        }
    }

    pub fn update(
        &mut self,
        chaperone_mgr: &mut ChaperoneSetupManager,
        overlays: &mut OverlayContainer<OpenVrOverlayData>,
        state: &AppState,
    ) {
        let universe = self.universe.clone();
        if let Some(data) = self.last.as_mut() {
            let pointer = &state.input_state.pointers[data.hand];
            if !pointer.now.space_drag {
                self.last = None;
                log::info!("End space drag");
                return;
            }

            let new_hand = data
                .pose
                .transform_point3a(state.input_state.pointers[data.hand].raw_pose.translation);
            let relative_pos = new_hand - data.hand_pos;

            if relative_pos.length_squared() > 1000.0 {
                log::warn!("Space drag too fast, ignoring");
                return;
            }

            let overlay_offset = data.pose.inverse().transform_vector3a(relative_pos);

            overlays.iter_mut().for_each(|overlay| {
                if overlay.state.grabbable {
                    overlay.state.dirty = true;
                    overlay.state.transform.translation -= overlay_offset;
                }
            });
            chaperone_mgr.revert_working_copy();
            apply_chaperone_offset(overlay_offset, chaperone_mgr);

            data.pose.translation += relative_pos;
            data.hand_pos = new_hand;

            set_working_copy(&universe, chaperone_mgr, &data.pose);
            chaperone_mgr.commit_working_copy(EChaperoneConfigFile::EChaperoneConfigFile_Live);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_drag {
                    let Some(mat) = get_working_copy(&universe, chaperone_mgr) else {
                        log::warn!("Can't space drag - failed to get zero pose");
                        return;
                    };
                    let hand_pos = mat.transform_point3a(pointer.raw_pose.translation);
                    self.last = Some(DragData {
                        pose: mat,
                        hand: i,
                        hand_pos,
                    });
                    log::info!("Start space drag");
                    break;
                }
            }
        }
    }

    pub fn reset_offset(&mut self, chaperone_mgr: &mut ChaperoneSetupManager) {
        set_working_copy(&self.universe, chaperone_mgr, &Affine3A::IDENTITY);
        chaperone_mgr.commit_working_copy(EChaperoneConfigFile::EChaperoneConfigFile_Live);

        if self.last.is_some() {
            log::info!("Space drag interrupted by manual reset");
            self.last = None;
        }
    }

    pub fn fix_floor(&mut self, chaperone_mgr: &mut ChaperoneSetupManager, input: &InputState) {
        let y1 = input.pointers[0].pose.translation.y;
        let y2 = input.pointers[1].pose.translation.y;
        let Some(mut mat) = get_working_copy(&self.universe, chaperone_mgr) else {
            log::warn!("Can't fix floor - failed to get zero pose");
            return;
        };
        mat.translation.y += y1.min(y2) - 0.03;
        set_working_copy(&self.universe, chaperone_mgr, &mat);
        chaperone_mgr.commit_working_copy(EChaperoneConfigFile::EChaperoneConfigFile_Live);
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

        if self.last.is_some() {
            log::info!("Space drag interrupted by external change");
            self.last = None;
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
    chaperone_mgr.revert_working_copy();
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
}

fn apply_chaperone_offset(offset: Vec3A, chaperone_mgr: &mut ChaperoneSetupManager) {
    let mut quads = chaperone_mgr.get_working_collision_bounds_info();
    quads.iter_mut().for_each(|quad| {
        quad.vCorners.iter_mut().for_each(|corner| {
            corner.v[0] += offset.x;
            if corner.v[1] != 0.0 {
                corner.v[1] += offset.y;
            }
            corner.v[2] += offset.z;
        });
    });
    chaperone_mgr.set_working_collision_bounds_info(&mut quads);
}
