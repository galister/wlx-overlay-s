mod client;
mod comp;
pub mod display;
pub mod egl_data;
mod egl_ex;
mod event_queue;
mod handle;
mod process;
mod smithay_wrapper;
mod time;
mod window;

use std::{cell::RefCell, rc::Rc};

use comp::Application;
use display::DisplayVec;
use event_queue::SyncEventQueue;
use process::ProcessVec;
use smallvec::SmallVec;
use smithay::{
    backend::renderer::gles::GlesRenderer,
    input::SeatState,
    reexports::wayland_server::{self, backend::ClientId},
    wayland::{
        compositor,
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelSurface, XdgShellState},
        shm::ShmState,
    },
};
use time::get_millis;

const STR_INVALID_HANDLE_DISP: &str = "Invalid display handle";
const STR_INVALID_HANDLE_PROCESS: &str = "Invalid process handle";

#[derive(Clone)]
pub struct WaylandEnv {
    pub display_num: u32,
}

impl WaylandEnv {
    pub fn display_num_string(&self) -> String {
        // e.g. "wayland-20"
        format!("wayland-{}", self.display_num)
    }
}

#[derive(Clone)]
pub enum WayVRTask {
    NewToplevel(ClientId, ToplevelSurface),
    ProcessTerminationRequest(process::ProcessHandle),
}

#[allow(dead_code)]
pub struct WayVR {
    time_start: u64,
    gles_renderer: GlesRenderer,
    pub displays: display::DisplayVec,
    manager: client::WayVRManager,
    wm: Rc<RefCell<window::WindowManager>>,
    egl_data: Rc<egl_data::EGLData>,
    pub processes: process::ProcessVec,

    tasks: SyncEventQueue<WayVRTask>,
}

pub enum MouseIndex {
    Left,
    Center,
    Right,
}

impl WayVR {
    pub fn new() -> anyhow::Result<Self> {
        let display: wayland_server::Display<Application> = wayland_server::Display::new()?;
        let dh = display.handle();
        let compositor = compositor::CompositorState::new::<Application>(&dh);
        let xdg_shell = XdgShellState::new::<Application>(&dh);
        let mut seat_state = SeatState::new();
        let shm = ShmState::new::<Application>(&dh, Vec::new());
        let data_device = DataDeviceState::new::<Application>(&dh);
        let mut seat = seat_state.new_wl_seat(&dh, "wayvr");

        // TODO: Keyboard repeat delay and rate?
        let seat_keyboard = seat.add_keyboard(Default::default(), 100, 100)?;
        let seat_pointer = seat.add_pointer();

        let tasks = SyncEventQueue::new();

        let state = Application {
            compositor,
            xdg_shell,
            seat_state,
            shm,
            data_device,
            wayvr_tasks: tasks.clone(),
        };

        let time_start = get_millis();
        let egl_data = egl_data::EGLData::new()?;
        let smithay_display = smithay_wrapper::get_egl_display(&egl_data)?;
        let smithay_context = smithay_wrapper::get_egl_context(&egl_data, &smithay_display)?;
        let gles_renderer = unsafe { GlesRenderer::new(smithay_context)? };

        Ok(Self {
            gles_renderer,
            time_start,
            manager: client::WayVRManager::new(state, display, seat_keyboard, seat_pointer)?,
            displays: DisplayVec::new(),
            processes: ProcessVec::new(),
            egl_data: Rc::new(egl_data),
            wm: Rc::new(RefCell::new(window::WindowManager::new())),
            tasks,
        })
    }

    pub fn tick_display(&mut self, display: display::DisplayHandle) -> anyhow::Result<()> {
        // millis since the start of wayvr
        let display = self
            .displays
            .get(&display)
            .ok_or(anyhow::anyhow!(STR_INVALID_HANDLE_DISP))?;

        let time_ms = get_millis() - self.time_start;

        if !display.visible {
            // Display is invisible, do not render
            return Ok(());
        }

        display.tick_render(&mut self.gles_renderer, time_ms)?;

        Ok(())
    }

    pub fn tick_events(&mut self) -> anyhow::Result<()> {
        // Tick all child processes
        let mut to_remove: SmallVec<[(process::ProcessHandle, display::DisplayHandle); 2]> =
            SmallVec::new();
        self.processes.iter_mut(&mut |handle, process| {
            if !process.is_running() {
                to_remove.push((handle, process.display_handle));
            }
        });

        for (p_handle, disp_handle) in to_remove {
            self.processes.remove(&p_handle);

            if let Some(display) = self.displays.get(&disp_handle) {
                display
                    .tasks
                    .send(display::DisplayTask::ProcessCleanup(p_handle));
            }
        }

        for display in self.displays.vec.iter_mut().flatten() {
            display.obj.tick();
        }

        while let Some(task) = self.tasks.read() {
            match task {
                WayVRTask::NewToplevel(client_id, toplevel) => {
                    // Attach newly created toplevel surfaces to displays
                    for client in &self.manager.clients {
                        if client.client.id() == client_id {
                            let window_handle = self.wm.borrow_mut().create_window(&toplevel);

                            if let Some(process_handle) =
                                process::find_by_pid(&self.processes, client.pid)
                            {
                                if let Some(display) = self.displays.get_mut(&client.display_handle)
                                {
                                    display.add_window(window_handle, process_handle, &toplevel);
                                } else {
                                    // This shouldn't happen, scream if it does
                                    log::error!("Could not attach window handle into display");
                                }
                            } else {
                                log::error!(
                                    "Failed to find process by PID {}. It was probably spawned externally.",
                                    client.pid
                                );
                            }

                            break;
                        }
                    }
                }
                WayVRTask::ProcessTerminationRequest(process_handle) => {
                    if let Some(process) = self.processes.get_mut(&process_handle) {
                        process.terminate();
                    }
                }
            }
        }

        self.manager
            .tick_wayland(&mut self.displays, &mut self.processes)
    }

    pub fn tick_finish(&mut self) -> anyhow::Result<()> {
        self.gles_renderer.with_context(|gl| unsafe {
            gl.Flush();
            gl.Finish();
        })?;
        Ok(())
    }

    pub fn send_mouse_move(&mut self, display: display::DisplayHandle, x: u32, y: u32) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_move(&mut self.manager, x, y);
        }
    }

    pub fn send_mouse_down(&mut self, display: display::DisplayHandle, index: MouseIndex) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_down(&mut self.manager, index);
        }
    }

    pub fn send_mouse_up(&mut self, display: display::DisplayHandle, index: MouseIndex) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_up(&mut self.manager, index);
        }
    }

    pub fn send_mouse_scroll(&mut self, display: display::DisplayHandle, delta: f32) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_scroll(&mut self.manager, delta);
        }
    }

    pub fn send_key(&mut self, virtual_key: u32, down: bool) {
        self.manager.send_key(virtual_key, down);
    }

    pub fn set_display_visible(&mut self, display: display::DisplayHandle, visible: bool) {
        if let Some(display) = self.displays.get_mut(&display) {
            display.set_visible(visible);
        }
    }

    pub fn get_dmabuf_data(&self, display: display::DisplayHandle) -> Option<egl_data::DMAbufData> {
        self.displays
            .get(&display)
            .map(|display| display.dmabuf_data.clone())
    }

    pub fn get_display_by_name(&self, name: &str) -> Option<display::DisplayHandle> {
        for (idx, cell) in self.displays.vec.iter().enumerate() {
            if let Some(cell) = cell {
                if cell.obj.name == name {
                    return Some(DisplayVec::get_handle(cell, idx));
                }
            }
        }
        None
    }
    pub fn create_display(
        &mut self,
        width: u32,
        height: u32,
        name: &str,
    ) -> anyhow::Result<display::DisplayHandle> {
        let display = display::Display::new(
            self.wm.clone(),
            &mut self.gles_renderer,
            self.egl_data.clone(),
            self.manager.wayland_env.clone(),
            width,
            height,
            name,
        )?;
        Ok(self.displays.add(display))
    }

    pub fn destroy_display(&mut self, handle: display::DisplayHandle) {
        self.displays.remove(&handle);
    }

    // Check if process with given arguments already exists
    pub fn process_query(
        &self,
        display_handle: display::DisplayHandle,
        exec_path: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> Option<process::ProcessHandle> {
        for (idx, cell) in self.processes.vec.iter().enumerate() {
            if let Some(cell) = &cell {
                let process = &cell.obj;
                if process.display_handle != display_handle
                    || process.exec_path != exec_path
                    || process.args != args
                {
                    continue;
                }

                return Some(process::ProcessVec::get_handle(cell, idx));
            }
        }

        None
    }

    pub fn terminate_process(&mut self, process_handle: process::ProcessHandle) {
        self.tasks
            .send(WayVRTask::ProcessTerminationRequest(process_handle));
    }

    pub fn spawn_process(
        &mut self,
        display_handle: display::DisplayHandle,
        exec_path: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> anyhow::Result<process::ProcessHandle> {
        let display = self
            .displays
            .get_mut(&display_handle)
            .ok_or(anyhow::anyhow!(STR_INVALID_HANDLE_DISP))?;

        let res = display.spawn_process(exec_path, args, env)?;
        Ok(self.processes.add(process::Process {
            auth_key: res.auth_key,
            child: res.child,
            display_handle,
            exec_path: String::from(exec_path),
            args: args.iter().map(|x| String::from(*x)).collect(),
            env: env
                .iter()
                .map(|(a, b)| (String::from(*a), String::from(*b)))
                .collect(),
        }))
    }
}
