use libmonado::{BlockFlags, ClientLogic, ClientState, Monado, Version};
use log::{trace, warn};

use crate::state::AppState;

pub(super) struct InputBlocker {
    use_io_blocks: bool,
    inputs_blocked_last_frame: bool,
    poses_blocked_last_frame: bool,
}

impl InputBlocker {
    pub fn new(monado: &Monado) -> Self {
        Self {
            use_io_blocks: monado.get_api_version() >= Version::new(1, 6, 0),
            inputs_blocked_last_frame: false,
            poses_blocked_last_frame: false,
        }
    }

    pub fn unblock(&self, monado: &mut Monado) {
        self.block_inputs(monado, false, false);
    }

    pub fn update(&mut self, app: &mut AppState) {
        let Some(monado) = &mut app.monado else {
            return; // monado not available
        };

        let should_block_inputs = app
            .input_state
            .pointers
            .iter()
            .any(|p| p.interaction.should_block_input)
            && app.session.config.block_game_input;

        let should_block_poses = app
            .input_state
            .pointers
            .iter()
            .any(|p| p.interaction.should_block_poses)
            && app.session.config.block_poses_on_kbd_interaction;

        if should_block_inputs != self.inputs_blocked_last_frame
            || should_block_poses != self.poses_blocked_last_frame
        {
            if should_block_inputs {
                trace!("Blocking input");
            } else {
                trace!("Unblocking input");
            }
            self.block_inputs(monado, should_block_inputs, should_block_poses);
        }

        self.inputs_blocked_last_frame = should_block_inputs;
        self.poses_blocked_last_frame = should_block_poses;
    }

    fn block_inputs(&self, monado: &mut Monado, block_inputs: bool, block_poses: bool) {
        match monado.clients() {
            Ok(clients) => {
                for mut client in clients {
                    match client.name() {
                        Ok(n) => {
                            if n == "wayvr" {
                                continue;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to get client name: {e}");
                            continue;
                        }
                    };

                    let state = match client.state() {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("Failed to get client state: {e}");
                            continue;
                        }
                    };

                    if state.contains(ClientState::ClientSessionVisible) {
                        let r = if self.use_io_blocks {
                            let flags = match (block_inputs, block_poses) {
                                (true, true) => BlockFlags::BlockPoses | BlockFlags::BlockInputs,
                                (true, false) => BlockFlags::BlockInputs.into(),
                                (false, _) => BlockFlags::None.into(),
                            };
                            client.set_io_blocks(flags)
                        } else {
                            client.set_io_active(!block_inputs)
                        };
                        if let Err(e) = r {
                            warn!("Failed to set io active for client: {e}");
                        }
                    }
                }
            }
            Err(e) => warn!("Failed to get clients from Monado: {e}"),
        }
    }
}
