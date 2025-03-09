pub mod client;
mod comp;
pub mod display;
pub mod egl_data;
mod egl_ex;
pub mod event_queue;
mod handle;
mod process;
pub mod server_ipc;
mod smithay_wrapper;
mod time;
mod window;
use comp::Application;
use display::{DisplayInitParams, DisplayVec};
use event_queue::SyncEventQueue;
use process::ProcessVec;
use server_ipc::WayVRServer;
use smallvec::SmallVec;
use smithay::{
    backend::{
        egl,
        renderer::{gles::GlesRenderer, ImportDma},
    },
    input::SeatState,
    output::{Mode, Output},
    reexports::wayland_server::{self, backend::ClientId},
    wayland::{
        compositor,
        dmabuf::{DmabufFeedbackBuilder, DmabufState},
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelSurface, XdgShellState},
        shm::ShmState,
    },
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
use time::get_millis;
use wayvr_ipc::{packet_client, packet_server};

use crate::{hid::MODS_TO_KEYS, state::AppState};

const STR_INVALID_HANDLE_DISP: &str = "Invalid display handle";
const STR_INVALID_HANDLE_PROCESS: &str = "Invalid process handle";

#[derive(Debug, Clone)]
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
pub struct ProcessWayVREnv {
    pub display_auth: Option<String>,
    pub display_name: Option<String>, // Externally spawned process by a user script
}

#[derive(Clone)]
pub struct ExternalProcessRequest {
    pub env: ProcessWayVREnv,
    pub client: wayland_server::Client,
    pub pid: u32,
}

#[derive(Clone)]
pub enum WayVRTask {
    NewToplevel(ClientId, ToplevelSurface),
    NewExternalProcess(ExternalProcessRequest),
    DropOverlay(super::overlay::OverlayID),
    ProcessTerminationRequest(process::ProcessHandle),
    Haptics(super::input::Haptics),
}

#[derive(Clone)]
pub enum WayVRSignal {
    DisplayVisibility(display::DisplayHandle, bool),
    DisplayWindowLayout(
        display::DisplayHandle,
        packet_server::WvrDisplayWindowLayout,
    ),
    BroadcastStateChanged(packet_server::WvrStateChanged),
}

pub struct Config {
    pub click_freeze_time_ms: u32,
    pub keyboard_repeat_delay_ms: u32,
    pub keyboard_repeat_rate: u32,
    pub auto_hide_delay: Option<u32>, // if None, auto-hide is disabled
}

pub struct WayVRState {
    time_start: u64,
    pub displays: display::DisplayVec,
    pub manager: client::WayVRCompositor,
    wm: Rc<RefCell<window::WindowManager>>,
    egl_data: Rc<egl_data::EGLData>,
    pub processes: process::ProcessVec,
    config: Config,
    dashboard_display: Option<display::DisplayHandle>,
    pub tasks: SyncEventQueue<WayVRTask>,
    pub signals: SyncEventQueue<WayVRSignal>,
    ticks: u64,
    pub pending_haptic: Option<super::input::Haptics>,
    cur_modifiers: u8,
}

pub struct WayVR {
    pub state: WayVRState,
    pub ipc_server: WayVRServer,
}

pub enum MouseIndex {
    Left,
    Center,
    Right,
}

pub enum TickTask {
    NewExternalProcess(ExternalProcessRequest), // Call WayVRCompositor::add_client after receiving this message
    NewDisplay(
        packet_client::WvrDisplayCreateParams,
        Option<display::DisplayHandle>, /* existing handle? */
    ),
    DropOverlay(super::overlay::OverlayID),
}

impl WayVR {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        log::info!("Initializing WayVR");
        let display: wayland_server::Display<Application> = wayland_server::Display::new()?;
        let dh = display.handle();
        let compositor = compositor::CompositorState::new::<Application>(&dh);
        let xdg_shell = XdgShellState::new::<Application>(&dh);
        let mut seat_state = SeatState::new();
        let shm = ShmState::new::<Application>(&dh, Vec::new());
        let data_device = DataDeviceState::new::<Application>(&dh);
        let mut seat = seat_state.new_wl_seat(&dh, "wayvr");

        let dummy_width = 1280;
        let dummy_height = 720;
        let dummy_milli_hz = 60000; /* refresh rate in millihertz */

        let output = Output::new(
            String::from("wayvr_display"),
            smithay::output::PhysicalProperties {
                size: (dummy_width, dummy_height).into(),
                subpixel: smithay::output::Subpixel::None,
                make: String::from("Completely Legit"),
                model: String::from("Virtual WayVR Display"),
            },
        );

        let mode = Mode {
            refresh: dummy_milli_hz,
            size: (dummy_width, dummy_height).into(),
        };

        let _global = output.create_global::<Application>(&dh);
        output.change_current_state(Some(mode), None, None, None);
        output.set_preferred(mode);

        let egl_data = egl_data::EGLData::new()?;

        let smithay_display = smithay_wrapper::get_egl_display(&egl_data)?;
        let smithay_context = smithay_wrapper::get_egl_context(&egl_data, &smithay_display)?;

        let render_node = egl::EGLDevice::device_for_display(&smithay_display)
            .and_then(|device| device.try_get_render_node());

        let gles_renderer = unsafe { GlesRenderer::new(smithay_context)? };

        let dmabuf_default_feedback = match render_node {
            Ok(Some(node)) => {
                let dmabuf_formats = gles_renderer.dmabuf_formats();
                let dmabuf_default_feedback =
                    DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                        .build()
                        .unwrap();
                Some(dmabuf_default_feedback)
            }
            Ok(None) => {
                log::warn!("dmabuf: Failed to query render node");
                None
            }
            Err(err) => {
                log::warn!("dmabuf: Failed to get egl device for display: {}", err);
                None
            }
        };

        let dmabuf_state = if let Some(default_feedback) = dmabuf_default_feedback {
            let mut dmabuf_state = DmabufState::new();
            let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<Application>(
                &display.handle(),
                &default_feedback,
            );
            (dmabuf_state, dmabuf_global, Some(default_feedback))
        } else {
            let dmabuf_formats = gles_renderer.dmabuf_formats();
            let mut dmabuf_state = DmabufState::new();
            let dmabuf_global =
                dmabuf_state.create_global::<Application>(&display.handle(), dmabuf_formats);
            (dmabuf_state, dmabuf_global, None)
        };

        let seat_keyboard = seat.add_keyboard(
            Default::default(),
            config.keyboard_repeat_delay_ms as i32,
            config.keyboard_repeat_rate as i32,
        )?;
        let seat_pointer = seat.add_pointer();

        let tasks = SyncEventQueue::new();

        let state = Application {
            compositor,
            xdg_shell,
            seat_state,
            shm,
            data_device,
            wayvr_tasks: tasks.clone(),
            redraw_requests: HashSet::new(),
            dmabuf_state,
            gles_renderer,
        };

        let time_start = get_millis();

        let ipc_server = WayVRServer::new()?;

        let state = WayVRState {
            time_start,
            manager: client::WayVRCompositor::new(state, display, seat_keyboard, seat_pointer)?,
            displays: DisplayVec::new(),
            processes: ProcessVec::new(),
            egl_data: Rc::new(egl_data),
            wm: Rc::new(RefCell::new(window::WindowManager::new())),
            config,
            dashboard_display: None,
            ticks: 0,
            tasks,
            pending_haptic: None,
            signals: SyncEventQueue::new(),
            cur_modifiers: 0,
        };

        Ok(Self { state, ipc_server })
    }

    pub fn tick_display(&mut self, display: display::DisplayHandle) -> anyhow::Result<()> {
        // millis since the start of wayvr
        let display = self
            .state
            .displays
            .get_mut(&display)
            .ok_or(anyhow::anyhow!(STR_INVALID_HANDLE_DISP))?;

        if !display.wants_redraw {
            // Nothing changed, do not render
            return Ok(());
        }

        if !display.visible {
            // Display is invisible, do not render
            return Ok(());
        }

        let time_ms = get_millis() - self.state.time_start;

        display.tick_render(&mut self.state.manager.state.gles_renderer, time_ms)?;
        display.wants_redraw = false;

        Ok(())
    }

    pub fn tick_events(&mut self, app: &AppState) -> anyhow::Result<Vec<TickTask>> {
        let mut tasks: Vec<TickTask> = Vec::new();

        self.ipc_server.tick(&mut server_ipc::TickParams {
            state: &mut self.state,
            tasks: &mut tasks,
            app,
        })?;

        // Check for redraw events
        self.state.displays.iter_mut(&mut |_, disp| {
            for disp_window in &disp.displayed_windows {
                if self
                    .state
                    .manager
                    .state
                    .check_redraw(disp_window.toplevel.wl_surface())
                {
                    disp.wants_redraw = true;
                }
            }
        });

        // Tick all child processes
        let mut to_remove: SmallVec<[(process::ProcessHandle, display::DisplayHandle); 2]> =
            SmallVec::new();

        self.state.processes.iter_mut(&mut |handle, process| {
            if !process.is_running() {
                to_remove.push((handle, process.display_handle()));
            }
        });

        for (p_handle, disp_handle) in &to_remove {
            self.state.processes.remove(p_handle);

            if let Some(display) = self.state.displays.get_mut(disp_handle) {
                display
                    .tasks
                    .send(display::DisplayTask::ProcessCleanup(*p_handle));
                display.wants_redraw = true;
            }
        }

        self.state.displays.iter_mut(&mut |handle, display| {
            display.tick(&self.state.config, &handle, &mut self.state.signals);
        });

        if !to_remove.is_empty() {
            self.state.signals.send(WayVRSignal::BroadcastStateChanged(
                packet_server::WvrStateChanged::ProcessRemoved,
            ));
        }

        while let Some(task) = self.state.tasks.read() {
            match task {
                WayVRTask::NewExternalProcess(req) => {
                    tasks.push(TickTask::NewExternalProcess(req));
                }
                WayVRTask::DropOverlay(overlay_id) => {
                    tasks.push(TickTask::DropOverlay(overlay_id));
                }
                WayVRTask::NewToplevel(client_id, toplevel) => {
                    // Attach newly created toplevel surfaces to displays
                    for client in &self.state.manager.clients {
                        if client.client.id() == client_id {
                            if let Some(process_handle) =
                                process::find_by_pid(&self.state.processes, client.pid)
                            {
                                let window_handle = self
                                    .state
                                    .wm
                                    .borrow_mut()
                                    .create_window(client.display_handle, &toplevel);

                                if let Some(display) =
                                    self.state.displays.get_mut(&client.display_handle)
                                {
                                    display.add_window(window_handle, process_handle, &toplevel);
                                    self.state.signals.send(WayVRSignal::BroadcastStateChanged(
                                        packet_server::WvrStateChanged::WindowCreated,
                                    ));
                                } else {
                                    // This shouldn't happen, scream if it does
                                    log::error!("Could not attach window handle into display");
                                }
                            } else {
                                log::error!(
                                    "WayVR window creation failed: Unexpected process ID {}. It wasn't registered before.",
                                    client.pid
                                );
                            }

                            break;
                        }
                    }
                }
                WayVRTask::ProcessTerminationRequest(process_handle) => {
                    if let Some(process) = self.state.processes.get_mut(&process_handle) {
                        process.terminate();
                    }
                }
                WayVRTask::Haptics(haptics) => {
                    self.state.pending_haptic = Some(haptics);
                }
            }
        }

        self.state
            .manager
            .tick_wayland(&mut self.state.displays, &mut self.state.processes)?;

        if self.state.ticks % 200 == 0 {
            self.state.manager.cleanup_clients();
        }

        self.state.ticks += 1;

        Ok(tasks)
    }

    pub fn tick_finish(&mut self) -> anyhow::Result<()> {
        self.state
            .manager
            .state
            .gles_renderer
            .with_context(|gl| unsafe {
                gl.Flush();
                gl.Finish();
            })?;
        Ok(())
    }

    pub fn get_primary_display(displays: &DisplayVec) -> Option<display::DisplayHandle> {
        for (idx, cell) in displays.vec.iter().enumerate() {
            if let Some(cell) = cell {
                if cell.obj.primary {
                    return Some(DisplayVec::get_handle(cell, idx));
                }
            }
        }
        None
    }

    pub fn get_display_by_name(
        displays: &DisplayVec,
        name: &str,
    ) -> Option<display::DisplayHandle> {
        for (idx, cell) in displays.vec.iter().enumerate() {
            if let Some(cell) = cell {
                if cell.obj.name == name {
                    return Some(DisplayVec::get_handle(cell, idx));
                }
            }
        }
        None
    }

    pub fn terminate_process(&mut self, process_handle: process::ProcessHandle) {
        self.state
            .tasks
            .send(WayVRTask::ProcessTerminationRequest(process_handle));
    }
}

impl WayVRState {
    pub fn send_mouse_move(&mut self, display: display::DisplayHandle, x: u32, y: u32) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_move(&self.config, &mut self.manager, x, y);
        }
    }

    pub fn send_mouse_down(&mut self, display: display::DisplayHandle, index: MouseIndex) {
        if let Some(display) = self.displays.get_mut(&display) {
            display.send_mouse_down(&mut self.manager, index);
        }
    }

    pub fn send_mouse_up(&mut self, display: display::DisplayHandle, index: MouseIndex) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_up(&mut self.manager, index);
        }
    }

    pub fn send_mouse_scroll(
        &mut self,
        display: display::DisplayHandle,
        delta_y: f32,
        delta_x: f32,
    ) {
        if let Some(display) = self.displays.get(&display) {
            display.send_mouse_scroll(&mut self.manager, delta_y, delta_x);
        }
    }

    pub fn send_key(&mut self, virtual_key: u32, down: bool) {
        self.manager.send_key(virtual_key, down);
    }

    pub fn set_modifiers(&mut self, modifiers: u8) {
        let changed = self.cur_modifiers ^ modifiers;
        for i in 0..8 {
            let m = 1 << i;
            if changed & m != 0 {
                if let Some(vk) = MODS_TO_KEYS.get(m).into_iter().flatten().next() {
                    self.send_key(*vk as u32, modifiers & m != 0);
                }
            }
        }
        self.cur_modifiers = modifiers;
    }

    pub fn set_display_visible(&mut self, display: display::DisplayHandle, visible: bool) {
        if let Some(display) = self.displays.get_mut(&display) {
            display.set_visible(visible);
        }
    }

    pub fn set_display_layout(
        &mut self,
        display: display::DisplayHandle,
        layout: packet_server::WvrDisplayWindowLayout,
    ) {
        if let Some(display) = self.displays.get_mut(&display) {
            display.set_layout(layout);
        }
    }

    pub fn get_dmabuf_data(&self, display: display::DisplayHandle) -> Option<egl_data::DMAbufData> {
        self.displays
            .get(&display)
            .map(|display| display.dmabuf_data.clone())
    }

    pub fn create_display(
        &mut self,
        width: u16,
        height: u16,
        name: &str,
        primary: bool,
    ) -> anyhow::Result<display::DisplayHandle> {
        let display = display::Display::new(DisplayInitParams {
            wm: self.wm.clone(),
            egl_data: self.egl_data.clone(),
            renderer: &mut self.manager.state.gles_renderer,
            wayland_env: self.manager.wayland_env.clone(),
            width,
            height,
            name,
            primary,
        })?;

        let handle = self.displays.add(display);

        self.signals.send(WayVRSignal::BroadcastStateChanged(
            packet_server::WvrStateChanged::DisplayCreated,
        ));

        Ok(handle)
    }

    pub fn destroy_display(&mut self, handle: display::DisplayHandle) -> anyhow::Result<()> {
        let Some(display) = self.displays.get(&handle) else {
            anyhow::bail!("Display not found");
        };

        if let Some(overlay_id) = display.overlay_id {
            self.tasks.send(WayVRTask::DropOverlay(overlay_id));
        } else {
            log::warn!("Destroying display without OverlayID set"); // This shouldn't happen, but log it anyways.
        }

        let mut process_names = Vec::<String>::new();

        self.processes.iter_mut(&mut |_, process| {
            if process.display_handle() == handle {
                process_names.push(process.get_name());
            }
        });

        if !display.displayed_windows.is_empty() || !process_names.is_empty() {
            anyhow::bail!(
                "Display is not empty. Attached processes: {}",
                process_names.join(", ")
            );
        }

        self.manager.cleanup_clients();

        for client in self.manager.clients.iter() {
            if client.display_handle == handle {
                // This shouldn't happen, but make sure we are all set to destroy this display
                anyhow::bail!("Wayland client still exists");
            }
        }

        self.displays.remove(&handle);

        self.signals.send(WayVRSignal::BroadcastStateChanged(
            packet_server::WvrStateChanged::DisplayRemoved,
        ));

        Ok(())
    }

    pub fn get_or_create_dashboard_display(
        &mut self,
        width: u16,
        height: u16,
        name: &str,
    ) -> anyhow::Result<(bool /* newly created? */, display::DisplayHandle)> {
        if let Some(handle) = &self.dashboard_display {
            // ensure it still exists
            if self.displays.get(handle).is_some() {
                return Ok((false, *handle));
            }
        }

        let new_disp = self.create_display(width, height, name, false)?;
        self.dashboard_display = Some(new_disp);

        Ok((true, new_disp))
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
                if let process::Process::Managed(process) = &cell.obj {
                    if process.display_handle != display_handle
                        || process.exec_path != exec_path
                        || process.args != args
                    {
                        continue;
                    }
                    return Some(process::ProcessVec::get_handle(cell, idx));
                }
            }
        }

        None
    }

    pub fn add_external_process(
        &mut self,
        display_handle: display::DisplayHandle,
        pid: u32,
    ) -> process::ProcessHandle {
        self.processes
            .add(process::Process::External(process::ExternalProcess {
                display_handle,
                pid,
            }))
    }

    pub fn spawn_process(
        &mut self,
        display_handle: display::DisplayHandle,
        exec_path: &str,
        args: &[&str],
        env: &[(&str, &str)],
        userdata: HashMap<String, String>,
    ) -> anyhow::Result<process::ProcessHandle> {
        let display = self
            .displays
            .get_mut(&display_handle)
            .ok_or(anyhow::anyhow!(STR_INVALID_HANDLE_DISP))?;

        let res = display.spawn_process(exec_path, args, env)?;

        let handle = self
            .processes
            .add(process::Process::Managed(process::WayVRProcess {
                auth_key: res.auth_key,
                child: res.child,
                display_handle,
                exec_path: String::from(exec_path),
                userdata,
                args: args.iter().map(|x| String::from(*x)).collect(),
                env: env
                    .iter()
                    .map(|(a, b)| (String::from(*a), String::from(*b)))
                    .collect(),
            }));

        self.signals.send(WayVRSignal::BroadcastStateChanged(
            packet_server::WvrStateChanged::ProcessCreated,
        ));

        Ok(handle)
    }
}
