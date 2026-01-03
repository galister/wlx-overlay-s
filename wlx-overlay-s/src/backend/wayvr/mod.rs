pub mod client;
mod comp;
mod handle;
mod image_importer;
pub mod process;
mod time;
pub mod window;
use anyhow::Context;
use comp::Application;
use process::ProcessVec;
use slotmap::SecondaryMap;
use smallvec::SmallVec;
use smithay::{
    desktop::PopupManager, input::{keyboard::XkbConfig, SeatState}, output::{Mode, Output}, reexports::wayland_server::{self, backend::ClientId}, wayland::{
        compositor::{self, with_states, SurfaceData},
        dmabuf::{DmabufFeedbackBuilder, DmabufState},
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelSurface, XdgShellState, XdgToplevelSurfaceData},
        shm::ShmState,
    }
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use time::get_millis;
use vulkano::image::view::ImageView;
use wayvr_ipc::packet_server;
use wgui::gfx::WGfx;
use wlx_capture::frame::Transform;
use xkbcommon::xkb;

use crate::{
    backend::{
        task::{OverlayTask, TaskType},
        wayvr::{image_importer::ImageImporter, window::Window},
    },
    graphics::WGfxExtras,
    ipc::{event_queue::SyncEventQueue, ipc_server, signal::WayVRSignal},
    overlays::wayvr::create_wl_window_overlay,
    state::AppState,
    subsystem::hid::{MODS_TO_KEYS, WheelDelta},
    windowing::{OverlayID, OverlaySelector},
};

const STR_INVALID_HANDLE_DISP: &str = "Invalid display handle";

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
    DropToplevel(ClientId, ToplevelSurface),
    NewExternalProcess(ExternalProcessRequest),
    ProcessTerminationRequest(process::ProcessHandle),
}

pub enum BlitMethod {
    Dmabuf,
    Software,
}

impl BlitMethod {
    pub fn from_string(str: &str) -> Option<Self> {
        match str {
            "dmabuf" => Some(Self::Dmabuf),
            "software" => Some(Self::Software),
            _ => None,
        }
    }
}

pub struct Config {
    pub click_freeze_time_ms: u32,
    pub keyboard_repeat_delay_ms: u32,
    pub keyboard_repeat_rate: u32,
    pub auto_hide_delay: Option<u32>, // if None, auto-hide is disabled
    pub blit_method: BlitMethod,
}

pub struct WvrServerState {
    time_start: u64,
    pub manager: client::WayVRCompositor,
    pub wm: window::WindowManager,
    pub processes: process::ProcessVec,
    pub config: Config,
    pub tasks: SyncEventQueue<WayVRTask>,
    ticks: u64,
    cur_modifiers: u8,
    signals: SyncEventQueue<WayVRSignal>,
    mouse_freeze: Instant,
    window_to_overlay: HashMap<window::WindowHandle, OverlayID>,
    overlay_to_window: SecondaryMap<OverlayID, window::WindowHandle>,
}

pub enum MouseIndex {
    Left,
    Center,
    Right,
}

pub enum TickTask {
    NewExternalProcess(ExternalProcessRequest), // Call WayVRCompositor::add_client after receiving this message
}

impl WvrServerState {
    pub fn new(
        gfx: Arc<WGfx>,
        gfx_extras: &WGfxExtras,
        config: Config,
        signals: SyncEventQueue<WayVRSignal>,
    ) -> anyhow::Result<Self> {
        log::info!("Initializing WayVR server");
        let display: wayland_server::Display<Application> = wayland_server::Display::new()?;
        let dh = display.handle();
        let compositor = compositor::CompositorState::new::<Application>(&dh);
        let xdg_shell = XdgShellState::new::<Application>(&dh);
        let mut seat_state = SeatState::new();
        let shm = ShmState::new::<Application>(&dh, Vec::new());
        let data_device = DataDeviceState::new::<Application>(&dh);
        let mut seat = seat_state.new_wl_seat(&dh, "wayvr");

        let dummy_width = 1920;
        let dummy_height = 1080;
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

        let main_device = {
            let (major, minor) = gfx_extras.drm_device.as_ref().context("No DRM device!")?;
            libc::makedev(*major as _, *minor as _)
        };

        // this will throw a compile-time error if smithay's drm-fourcc is out of sync with wlx-capture's
        let mut formats: Vec<smithay::backend::allocator::Format> = vec![];

        for f in &gfx_extras.drm_formats {
            formats.push(*f);
        }

        let dmabuf_state = DmabufFeedbackBuilder::new(main_device, formats.clone())
            .build()
            .map_or_else(
                |_| {
                    log::info!("Falling back to zwp_linux_dmabuf_v1 version 3.");
                    let mut dmabuf_state = DmabufState::new();
                    let dmabuf_global =
                        dmabuf_state.create_global::<Application>(&display.handle(), formats);
                    (dmabuf_state, dmabuf_global, None)
                },
                |default_feedback| {
                    let mut dmabuf_state = DmabufState::new();
                    let dmabuf_global = dmabuf_state
                        .create_global_with_default_feedback::<Application>(
                            &display.handle(),
                            &default_feedback,
                        );
                    (dmabuf_state, dmabuf_global, Some(default_feedback))
                },
            );

        let seat_keyboard = seat.add_keyboard(
            XkbConfig::default(),
            config.keyboard_repeat_delay_ms as i32,
            config.keyboard_repeat_rate as i32,
        )?;
        let seat_pointer = seat.add_pointer();

        let tasks = SyncEventQueue::new();

        let dma_importer = ImageImporter::new(gfx);

        let state = Application {
            image_importer: dma_importer,
            compositor,
            xdg_shell,
            seat_state,
            shm,
            data_device,
            wayvr_tasks: tasks.clone(),
            redraw_requests: HashSet::new(),
            dmabuf_state,
            popup_manager: PopupManager::default(),
        };

        let time_start = get_millis();

        Ok(Self {
            time_start,
            manager: client::WayVRCompositor::new(state, display, seat_keyboard, seat_pointer)?,
            processes: ProcessVec::new(),
            wm: window::WindowManager::new(),
            config,
            ticks: 0,
            tasks,
            cur_modifiers: 0,
            signals,
            mouse_freeze: Instant::now(),
            window_to_overlay: HashMap::new(),
            overlay_to_window: SecondaryMap::new(),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn tick_events(app: &mut AppState) -> anyhow::Result<Vec<TickTask>> {
        let mut tasks: Vec<TickTask> = Vec::new();

        let Some(wvr_server) = app.wvr_server.as_mut() else {
            return Ok(tasks);
        };

        app.ipc_server.tick(&mut ipc_server::TickParams {
            wvr_server,
            input_state: &app.input_state,
            tasks: &mut tasks,
            signals: &app.wayvr_signals,
        });

        // Tick all child processes
        let mut to_remove: SmallVec<[process::ProcessHandle; 2]> = SmallVec::new();

        for (handle, process) in wvr_server.processes.iter_mut() {
            if !process.is_running() {
                to_remove.push(handle);
            }
        }

        for p_handle in &to_remove {
            wvr_server.processes.remove(p_handle);
        }

        if !to_remove.is_empty() {
            app.wayvr_signals.send(WayVRSignal::BroadcastStateChanged(
                packet_server::WvrStateChanged::ProcessRemoved,
            ));
        }

        while let Some(task) = wvr_server.tasks.read() {
            match task {
                WayVRTask::NewExternalProcess(req) => {
                    tasks.push(TickTask::NewExternalProcess(req));
                }
                WayVRTask::NewToplevel(client_id, toplevel) => {
                    let toplevel = Rc::new(toplevel);

                    // Attach newly created toplevel surfaces to displays
                    for client in &wvr_server.manager.clients {
                        if client.client.id() != client_id {
                            continue;
                        }

                        let Some(process_handle) =
                            process::find_by_pid(&wvr_server.processes, client.pid)
                        else {
                            log::error!(
                                "WayVR window creation failed: Unexpected process ID {}. It wasn't registered before.",
                                client.pid
                            );
                            continue;
                        };

                        let window_handle = wvr_server
                            .wm
                            .create_window(toplevel.clone(), process_handle);

                        let title: Arc<str> = with_states(toplevel.wl_surface(), |states| {
                            states
                                .data_map
                                .get::<XdgToplevelSurfaceData>()
                                .and_then(|t| t.lock().unwrap().title.clone())
                                .map_or_else(|| format!("P{}", client.pid).into(), String::into)
                        });

                        app.tasks.enqueue(TaskType::Overlay(OverlayTask::Create(
                            OverlaySelector::Nothing,
                            Box::new(move |app: &mut AppState| {
                                create_wl_window_overlay(
                                    title,
                                    app,
                                    window_handle,
                                ).context("Could not create WvrWindow overlay").inspect_err(|e| log::warn!("{e:?}")).ok()
                            }),
                        )));

                        app.wayvr_signals.send(WayVRSignal::BroadcastStateChanged(
                            packet_server::WvrStateChanged::WindowCreated,
                        ));
                    }
                }
                WayVRTask::DropToplevel(client_id, toplevel) => {
                    for client in &wvr_server.manager.clients {
                        if client.client.id() != client_id {
                            continue;
                        }

                        let Some(window_handle) = wvr_server.wm.find_window_handle(&toplevel)
                        else {
                            log::warn!("DropToplevel: Couldn't find matching window handle");
                            continue;
                        };

                        if let Some(oid) = wvr_server.window_to_overlay.remove(&window_handle) {
                            app.tasks.enqueue(TaskType::Overlay(OverlayTask::Drop(
                                OverlaySelector::Id(oid),
                            )));
                            wvr_server.overlay_to_window.remove(oid);
                        }

                        wvr_server.wm.remove_window(window_handle);
                    }
                }
                WayVRTask::ProcessTerminationRequest(process_handle) => {
                    if let Some(process) = wvr_server.processes.get_mut(&process_handle) {
                        process.terminate();
                    }

                    //TODO: force drop related overlays
                }
            }
        }

        wvr_server.manager.tick_wayland(&mut wvr_server.processes)?;

        if wvr_server.ticks.is_multiple_of(200) {
            wvr_server.manager.cleanup_clients();
            wvr_server.manager.cleanup_handles();
        }

        wvr_server.ticks += 1;

        Ok(tasks)
    }

    pub fn terminate_process(&mut self, process_handle: process::ProcessHandle) {
        self.tasks
            .send(WayVRTask::ProcessTerminationRequest(process_handle));
    }

    pub fn overlay_added(&mut self, oid: OverlayID, window: window::WindowHandle) {
        self.overlay_to_window.insert(oid, window);
        self.window_to_overlay.insert(window, oid);
    }

    pub fn get_overlay_id(&self, window: window::WindowHandle) -> Option<OverlayID> {
        self.window_to_overlay.get(&window).cloned()
    }

    pub fn send_mouse_move(&mut self, handle: window::WindowHandle, x: u32, y: u32) {
        if self.mouse_freeze > Instant::now() {
            return;
        }
        if let Some(window) = self.wm.windows.get_mut(&handle) {
            window.send_mouse_move(&mut self.manager, x, y);
        } else {
            return;
        }
        self.mouse_freeze = Instant::now() + Duration::from_millis(1); // prevent other pointer from moving the mouse on the same frame
        self.wm.mouse = Some(window::MouseState {
            hover_window: handle,
            x,
            y,
        });
    }

    pub fn send_mouse_down(&mut self, handle: window::WindowHandle, index: MouseIndex) {
        self.mouse_freeze =
            Instant::now() + Duration::from_millis(self.config.click_freeze_time_ms as _);

        if let Some(window) = self.wm.windows.get_mut(&handle) {
            window.send_mouse_down(&mut self.manager, index);
        }
    }

    pub fn send_mouse_up(&mut self, index: MouseIndex) {
        Window::send_mouse_up(&mut self.manager, index);
    }

    pub fn send_mouse_scroll(&mut self, delta: WheelDelta) {
        Window::send_mouse_scroll(&mut self.manager, delta);
    }

    pub fn send_key(&mut self, virtual_key: u32, down: bool) {
        self.manager.send_key(virtual_key, down);
    }

    pub fn set_keymap(&mut self, keymap: &xkb::Keymap) -> anyhow::Result<()> {
        self.manager.set_keymap(keymap)
    }

    pub fn set_modifiers(&mut self, modifiers: u8) {
        let changed = self.cur_modifiers ^ modifiers;
        for i in 0..8 {
            let m = 1 << i;
            if changed & m != 0
                && let Some(vk) = MODS_TO_KEYS.get(m).into_iter().flatten().next()
            {
                self.send_key(*vk as u32, modifiers & m != 0);
            }
        }
        self.cur_modifiers = modifiers;
    }

    // Check if process with given arguments already exists
    pub fn process_query(
        &self,
        exec_path: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> Option<process::ProcessHandle> {
        for (idx, cell) in self.processes.vec.iter().enumerate() {
            if let Some(cell) = &cell
                && let process::Process::Managed(process) = &cell.obj
            {
                if process.exec_path != exec_path || process.args != args {
                    continue;
                }
                return Some(process::ProcessVec::get_handle(cell, idx));
            }
        }

        None
    }

    pub fn add_external_process(&mut self, pid: u32) -> process::ProcessHandle {
        self.processes
            .add(process::Process::External(process::ExternalProcess { pid }))
    }

    pub fn spawn_process(
        &mut self,
        exec_path: &str,
        args: &[&str],
        env: &[(&str, &str)],
        working_dir: Option<&str>,
        userdata: HashMap<String, String>,
    ) -> anyhow::Result<process::ProcessHandle> {
        let auth_key = generate_auth_key();

        let mut cmd = std::process::Command::new(exec_path);
        self.configure_env(&mut cmd, auth_key.as_str());
        cmd.args(args);
        if let Some(working_dir) = working_dir {
            cmd.current_dir(working_dir);
        }

        for e in env {
            cmd.env(e.0, e.1);
        }

        let child = cmd.spawn().context("Failed to spawn child process")?;

        let handle = self
            .processes
            .add(process::Process::Managed(process::WayVRProcess {
                auth_key,
                child,
                exec_path: String::from(exec_path),
                userdata,
                args: args.iter().map(|x| String::from(*x)).collect(),
                working_dir: working_dir.map(String::from),
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

    fn configure_env(&self, cmd: &mut std::process::Command, auth_key: &str) {
        cmd.env_remove("DISPLAY"); // Goodbye X11
        cmd.env(
            "WAYLAND_DISPLAY",
            self.manager.wayland_env.display_num_string(),
        );
        cmd.env("WAYVR_DISPLAY_AUTH", auth_key);
    }
}

fn generate_auth_key() -> String {
    let uuid = uuid::Uuid::new_v4();
    uuid.to_string()
}

pub struct SpawnProcessResult {
    pub auth_key: String,
    pub child: std::process::Child,
}

struct SurfaceBufWithImageContainer {
    inner: RefCell<SurfaceBufWithImage>,
}

#[derive(Clone)]
pub struct SurfaceBufWithImage {
    pub image: Arc<ImageView>,
    pub transform: Transform,
    pub scale: i32,
    pub dmabuf: bool,
}

impl SurfaceBufWithImage {
    fn apply_to_surface(self, surface_data: &SurfaceData) {
        if let Some(container) = surface_data.data_map.get::<SurfaceBufWithImageContainer>() {
            container.inner.replace(self);
        } else {
            surface_data
                .data_map
                .insert_if_missing(|| SurfaceBufWithImageContainer {
                    inner: RefCell::new(self),
                });
        }
    }

    pub fn get_from_surface(surface_data: &SurfaceData) -> Option<Self> {
        surface_data
            .data_map
            .get::<SurfaceBufWithImageContainer>()
            .map(|x| x.inner.borrow().clone())
    }
}
