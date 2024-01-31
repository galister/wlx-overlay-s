use glam::Vec3A;
use ovr_overlay::{chaperone_setup::ChaperoneSetupManager, sys::EChaperoneConfigFile};

use crate::{backend::common::OverlayContainer, state::AppState};

use super::overlay::OpenVrOverlayData;

pub(super) struct PlayspaceMover {
    drag_hand: Option<usize>,
    offset: Vec3A,
    start_position: Vec3A,
}

impl PlayspaceMover {
    pub fn new() -> Self {
        Self {
            drag_hand: None,
            offset: Vec3A::ZERO,
            start_position: Vec3A::ZERO,
        }
    }

    pub fn update(
        &mut self,
        chaperone_mgr: &mut ChaperoneSetupManager,
        overlays: &mut OverlayContainer<OpenVrOverlayData>,
        state: &AppState,
    ) {
        if let Some(hand) = self.drag_hand {
            let pointer = &state.input_state.pointers[hand];
            if !pointer.now.space_drag {
                self.drag_hand = None;
                return;
            }

            let hand_pos = state.input_state.pointers[hand].pose.translation;

            overlays.iter_mut().for_each(|overlay| {
                if overlay.state.grabbable {
                    overlay.state.transform.translation += hand_pos * -1.0;
                }
            });

            self.offset += hand_pos;
            self.apply_offset(chaperone_mgr);
        } else {
            for (i, pointer) in state.input_state.pointers.iter().enumerate() {
                if pointer.now.space_drag {
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

    fn apply_offset(&mut self, chaperone_mgr: &mut ChaperoneSetupManager) {
        let Some(mut zero_pose) =
            chaperone_mgr.get_working_standing_zero_pose_to_raw_tracking_pose()
        else {
            log::warn!("Can't space drag - failed to get zero pose");
            return;
        };

        zero_pose.m[0][3] = self.offset.x;
        zero_pose.m[1][3] = self.offset.y;
        zero_pose.m[2][3] = self.offset.z;

        chaperone_mgr.set_working_standing_zero_pose_to_raw_tracking_pose(&zero_pose);
        chaperone_mgr.commit_working_copy(EChaperoneConfigFile::EChaperoneConfigFile_Live);
    }
}
