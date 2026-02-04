use libmonado::{BlockFlags, ClientLogic, ClientState, Monado, Version};
use log::{trace, warn};

use crate::state::AppState;

pub(super) struct InputBlocker {
    use_io_blocks: bool,
    blocked_last_frame: bool,
}

impl InputBlocker {
    pub fn new(monado: &Monado) -> Self {
        Self {
            use_io_blocks: monado.get_api_version() >= Version::new(1, 6, 0),
            blocked_last_frame: false,
        }
    }

    pub fn update(&mut self, app: &mut AppState) {
        let Some(monado) = &mut app.monado else {
            return; // monado not available
        };

        let should_block = app
            .input_state
            .pointers
            .iter()
            .any(|p| p.interaction.should_block_input)
            && app.session.config.block_game_input;

        match (should_block, self.blocked_last_frame) {
            (true, false) => {
                trace!("Blocking input");
                self.block_inputs(monado, true);
            }
            (false, true) => {
                trace!("Unblocking input");
                self.block_inputs(monado, false);
            }
            _ => {}
        }

        self.blocked_last_frame = should_block;
    }

    fn block_inputs(&self, monado: &mut Monado, block: bool) {
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
                            client.set_io_blocks(if block {
                                BlockFlags::BlockInputs.into()
                            } else {
                                BlockFlags::None.into()
                            })
                        } else {
                            client.set_io_active(!block)
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
