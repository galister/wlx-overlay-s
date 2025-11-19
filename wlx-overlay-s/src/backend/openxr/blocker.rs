use libmonado::{ClientState, Monado};
use log::{trace, warn};

use crate::{state::AppState, windowing::OverlayID};

pub(super) struct InputBlocker {
    hovered_last_frame: bool,
}

impl InputBlocker {
    pub const fn new() -> Self {
        Self {
            hovered_last_frame: false,
        }
    }

    pub fn update(&mut self, state: &AppState, watch_id: OverlayID, monado: &mut Monado) {
        if !state.session.config.block_game_input {
            return;
        }

        let any_hovered = state.input_state.pointers.iter().any(|p| {
            p.interaction.hovered_id.is_some_and(|id| {
                id != watch_id || !state.session.config.block_game_input_ignore_watch
            })
        });

        match (any_hovered, self.hovered_last_frame) {
            (true, false) => {
                trace!("Blocking input");
                set_clients_io_active(monado, false);
            }
            (false, true) => {
                trace!("Unblocking input");
                set_clients_io_active(monado, true);
            }
            _ => {}
        }

        self.hovered_last_frame = any_hovered;
    }
}

fn set_clients_io_active(monado: &mut Monado, active: bool) {
    match monado.clients() {
        Ok(clients) => {
            for mut client in clients {
                let name = match client.name() {
                    Ok(n) => n,
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

                if name != "wlx-overlay-s"
                    && state.contains(ClientState::ClientSessionVisible)
                    && let Err(e) = client.set_io_active(active)
                {
                    warn!("Failed to set io active for client: {e}");
                }
            }
        }
        Err(e) => warn!("Failed to get clients from Monado: {e}"),
    }
}
