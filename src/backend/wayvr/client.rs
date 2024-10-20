use std::{io::Read, os::unix::net::UnixStream, sync::Arc};

use smithay::{
    backend::input::Keycode,
    input::{keyboard::KeyboardHandle, pointer::PointerHandle},
    reexports::wayland_server,
    utils::SerialCounter,
};

use super::{
    comp::{self},
    display, process,
};

pub struct WayVRClient {
    pub client: wayland_server::Client,
    pub display_handle: display::DisplayHandle,
    pub pid: u32,
}

pub struct WayVRManager {
    pub state: comp::Application,
    pub seat_keyboard: KeyboardHandle<comp::Application>,
    pub seat_pointer: PointerHandle<comp::Application>,
    pub serial_counter: SerialCounter,
    pub wayland_env: super::WaylandEnv,

    display: wayland_server::Display<comp::Application>,
    listener: wayland_server::ListeningSocket,

    toplevel_surf_count: u32, // for logging purposes

    pub clients: Vec<WayVRClient>,
}

fn get_display_auth_from_pid(pid: i32) -> anyhow::Result<String> {
    let path = format!("/proc/{}/environ", pid);
    let mut env_data = String::new();
    std::fs::File::open(path)?.read_to_string(&mut env_data)?;

    let lines: Vec<&str> = env_data.split('\0').filter(|s| !s.is_empty()).collect();

    for line in lines {
        if let Some((key, value)) = line.split_once('=') {
            if key == "WAYVR_DISPLAY_AUTH" {
                return Ok(String::from(value));
            }
        }
    }

    anyhow::bail!("Failed to get display auth from PID {}", pid);
}

impl WayVRManager {
    pub fn new(
        state: comp::Application,
        display: wayland_server::Display<comp::Application>,
        seat_keyboard: KeyboardHandle<comp::Application>,
        seat_pointer: PointerHandle<comp::Application>,
    ) -> anyhow::Result<Self> {
        let (wayland_env, listener) = create_wayland_listener()?;

        Ok(Self {
            state,
            display,
            seat_keyboard,
            seat_pointer,
            listener,
            wayland_env,
            serial_counter: SerialCounter::new(),
            clients: Vec::new(),
            toplevel_surf_count: 0,
        })
    }

    fn accept_connection(
        &mut self,
        stream: UnixStream,
        displays: &mut display::DisplayVec,
        processes: &mut process::ProcessVec,
    ) -> anyhow::Result<()> {
        let client = self
            .display
            .handle()
            .insert_client(stream, Arc::new(comp::ClientState::default()))
            .unwrap();

        let creds = client.get_credentials(&self.display.handle())?;
        let auth_key = get_display_auth_from_pid(creds.pid)?;

        // Find suitable auth key from the process list
        for process in processes.vec.iter().flatten() {
            let process = &process.obj;

            // Find process with matching auth key
            if process.auth_key.as_str() == auth_key {
                // Check if display handle is valid
                if displays.get(&process.display_handle).is_some() {
                    // Add client
                    self.clients.push(WayVRClient {
                        client,
                        display_handle: process.display_handle,
                        pid: creds.pid as u32,
                    });
                    return Ok(());
                }
            }
        }
        anyhow::bail!("Process auth key is invalid or selected display is non-existent");
    }

    fn accept_connections(
        &mut self,
        displays: &mut display::DisplayVec,
        processes: &mut process::ProcessVec,
    ) -> anyhow::Result<()> {
        if let Some(stream) = self.listener.accept()? {
            if let Err(e) = self.accept_connection(stream, displays, processes) {
                log::error!("Failed to accept connection: {}", e);
            }
        }

        Ok(())
    }

    pub fn tick_wayland(
        &mut self,
        displays: &mut display::DisplayVec,
        processes: &mut process::ProcessVec,
    ) -> anyhow::Result<()> {
        if let Err(e) = self.accept_connections(displays, processes) {
            log::error!("accept_connections failed: {}", e);
        }

        self.display.dispatch_clients(&mut self.state)?;
        self.display.flush_clients()?;

        let surf_count = self.state.xdg_shell.toplevel_surfaces().len() as u32;
        if surf_count != self.toplevel_surf_count {
            self.toplevel_surf_count = surf_count;
            log::info!("Toplevel surface count changed: {}", surf_count);
        }

        Ok(())
    }

    pub fn send_key(&mut self, virtual_key: u32, down: bool) {
        let state = if down {
            smithay::backend::input::KeyState::Pressed
        } else {
            smithay::backend::input::KeyState::Released
        };

        self.seat_keyboard.input::<(), _>(
            &mut self.state,
            Keycode::new(virtual_key),
            state,
            self.serial_counter.next_serial(),
            0,
            |_, _, _| smithay::input::keyboard::FilterResult::Forward,
        );
    }
}

const STARTING_WAYLAND_ADDR_IDX: u32 = 20;

fn create_wayland_listener() -> anyhow::Result<(super::WaylandEnv, wayland_server::ListeningSocket)>
{
    let mut env = super::WaylandEnv {
        display_num: STARTING_WAYLAND_ADDR_IDX,
    };

    let listener = loop {
        let display_str = env.display_num_string();
        log::debug!("Trying to open socket \"{}\"", display_str);
        match wayland_server::ListeningSocket::bind(display_str.as_str()) {
            Ok(listener) => {
                log::debug!("Listening to {}", display_str);
                break listener;
            }
            Err(e) => {
                log::debug!(
                    "Failed to open socket \"{}\" (reason: {}), trying next...",
                    display_str,
                    e
                );

                env.display_num += 1;
                if env.display_num > STARTING_WAYLAND_ADDR_IDX + 20 {
                    // Highly unlikely for the user to have 20 Wayland displays enabled at once. Return error instead.
                    anyhow::bail!("Failed to create wayland-server socket")
                }
            }
        }
    };

    Ok((env, listener))
}
