use libmonado::{ClientState, Monado};
use log::{trace, warn};

use crate::{state::AppState, windowing::OverlayID};

pub(super) struct InputBlocker {
    blocked_last_frame: bool,
}

impl InputBlocker {
    pub const fn new() -> Self {
        Self {
            blocked_last_frame: false,
        }
    }

    pub fn update(&mut self, app: &mut AppState, watch_id: OverlayID) {
        let Some(monado) = &mut app.monado else {
            return; // monado not available
        };

        let should_block = app.input_state.pointers.iter().any(|p| {
            p.interaction.hovered_id.is_some_and(|id| {
                id != watch_id || !app.session.config.block_game_input_ignore_watch
            })
        }) && app.session.config.block_game_input;

        match (should_block, self.blocked_last_frame) {
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

        self.blocked_last_frame = should_block;
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

                if name != "wayvr"
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
