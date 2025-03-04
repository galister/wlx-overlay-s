use std::{cell::RefCell, rc::Rc};

use smithay::{
    backend::renderer::{
        element::{
            surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
            Kind,
        },
        gles::{ffi, GlesRenderer, GlesTexture},
        utils::draw_render_elements,
        Bind, Color32F, Frame, Renderer,
    },
    input,
    utils::{Logical, Point, Rectangle, Size, Transform},
    wayland::shell::xdg::ToplevelSurface,
};
use wayvr_ipc::packet_server;

use crate::{
    backend::{overlay::OverlayID, wayvr::time::get_millis},
    gen_id,
};

use super::{
    client::WayVRCompositor, comp::send_frames_surface_tree, egl_data, event_queue::SyncEventQueue,
    process, smithay_wrapper, time, window, WayVRSignal,
};

fn generate_auth_key() -> String {
    let uuid = uuid::Uuid::new_v4();
    uuid.to_string()
}

#[derive(Debug)]
pub struct DisplayWindow {
    pub window_handle: window::WindowHandle,
    pub toplevel: ToplevelSurface,
    pub process_handle: process::ProcessHandle,
}

pub struct SpawnProcessResult {
    pub auth_key: String,
    pub child: std::process::Child,
}

#[derive(Debug)]
pub enum DisplayTask {
    ProcessCleanup(process::ProcessHandle),
}

const MAX_DISPLAY_SIZE: u16 = 8192;

#[derive(Debug)]
pub struct Display {
    // Display info stuff
    pub width: u16,
    pub height: u16,
    pub name: String,
    pub visible: bool,
    pub layout: packet_server::WvrDisplayWindowLayout,
    pub overlay_id: Option<OverlayID>,
    pub wants_redraw: bool,
    pub primary: bool,
    pub wm: Rc<RefCell<window::WindowManager>>,
    pub displayed_windows: Vec<DisplayWindow>,
    wayland_env: super::WaylandEnv,
    last_pressed_time_ms: u64,
    pub no_windows_since: Option<u64>,

    // Render data stuff
    gles_texture: GlesTexture, // TODO: drop texture
    egl_image: khronos_egl::Image,
    egl_data: Rc<egl_data::EGLData>,
    pub dmabuf_data: egl_data::DMAbufData,

    pub tasks: SyncEventQueue<DisplayTask>,
}

impl Drop for Display {
    fn drop(&mut self) {
        let _ = self
            .egl_data
            .egl
            .destroy_image(self.egl_data.display, self.egl_image);
    }
}

pub struct DisplayInitParams<'a> {
    pub wm: Rc<RefCell<window::WindowManager>>,
    pub renderer: &'a mut GlesRenderer,
    pub egl_data: Rc<egl_data::EGLData>,
    pub wayland_env: super::WaylandEnv,
    pub width: u16,
    pub height: u16,
    pub name: &'a str,
    pub primary: bool,
}

impl Display {
    pub fn new(params: DisplayInitParams) -> anyhow::Result<Self> {
        if params.width > MAX_DISPLAY_SIZE {
            anyhow::bail!(
                "display width ({}) is larger than {}",
                params.width,
                MAX_DISPLAY_SIZE
            );
        }

        if params.height > MAX_DISPLAY_SIZE {
            anyhow::bail!(
                "display height ({}) is larger than {}",
                params.height,
                MAX_DISPLAY_SIZE
            );
        }

        let tex_format = ffi::RGBA;
        let internal_format = ffi::RGBA8;

        let tex_id = params.renderer.with_context(|gl| {
            smithay_wrapper::create_framebuffer_texture(
                gl,
                params.width as u32,
                params.height as u32,
                tex_format,
                internal_format,
            )
        })?;

        let egl_image = params.egl_data.create_egl_image(tex_id)?;
        let dmabuf_data = params.egl_data.create_dmabuf_data(&egl_image)?;

        let opaque = false;
        let size = (params.width as i32, params.height as i32).into();
        let gles_texture = unsafe {
            GlesTexture::from_raw(params.renderer, Some(tex_format), opaque, tex_id, size)
        };

        Ok(Self {
            egl_data: params.egl_data,
            width: params.width,
            height: params.height,
            name: String::from(params.name),
            primary: params.primary,
            wayland_env: params.wayland_env,
            wm: params.wm,
            displayed_windows: Vec::new(),
            dmabuf_data,
            egl_image,
            gles_texture,
            last_pressed_time_ms: 0,
            no_windows_since: None,
            overlay_id: None,
            tasks: SyncEventQueue::new(),
            visible: true,
            wants_redraw: true,
            layout: packet_server::WvrDisplayWindowLayout::Tiling,
        })
    }

    pub fn as_packet(&self, handle: DisplayHandle) -> packet_server::WvrDisplay {
        packet_server::WvrDisplay {
            width: self.width,
            height: self.height,
            name: self.name.clone(),
            visible: self.visible,
            handle: handle.as_packet(),
        }
    }

    pub fn add_window(
        &mut self,
        window_handle: window::WindowHandle,
        process_handle: process::ProcessHandle,
        toplevel: &ToplevelSurface,
    ) {
        log::debug!("Attaching toplevel surface into display");
        self.displayed_windows.push(DisplayWindow {
            window_handle,
            process_handle,
            toplevel: toplevel.clone(),
        });
        self.reposition_windows();
    }

    pub fn reposition_windows(&mut self) {
        let window_count = self.displayed_windows.len();

        match &self.layout {
            packet_server::WvrDisplayWindowLayout::Tiling => {
                let mut i = 0;
                for win in self.displayed_windows.iter_mut() {
                    if let Some(window) = self.wm.borrow_mut().windows.get_mut(&win.window_handle) {
                        if !window.visible {
                            continue;
                        }
                        let d_cur = i as f32 / window_count as f32;
                        let d_next = (i + 1) as f32 / window_count as f32;

                        let left = (d_cur * self.width as f32) as i32;
                        let right = (d_next * self.width as f32) as i32;

                        window.set_pos(left, 0);
                        window.set_size((right - left) as u32, self.height as u32);
                        i += 1;
                    }
                }
            }
            packet_server::WvrDisplayWindowLayout::Stacking(opts) => {
                let do_margins = |margins: &packet_server::Margins, window: &mut window::Window| {
                    let top = margins.top as i32;
                    let bottom = self.height as i32 - margins.bottom as i32;
                    let left = margins.left as i32;
                    let right = self.width as i32 - margins.right as i32;
                    let width = right - left;
                    let height = bottom - top;
                    if width < 0 || height < 0 {
                        return; // wrong parameters, do nothing!
                    }

                    window.set_pos(left, top);
                    window.set_size(width as u32, height as u32);
                };

                let mut i = 0;
                for win in self.displayed_windows.iter_mut() {
                    if let Some(window) = self.wm.borrow_mut().windows.get_mut(&win.window_handle) {
                        if !window.visible {
                            continue;
                        }
                        do_margins(
                            if i == 0 {
                                &opts.margins_first
                            } else {
                                &opts.margins_rest
                            },
                            window,
                        );
                        i += 1;
                    }
                }
            }
        }
    }

    pub fn tick(
        &mut self,
        config: &super::Config,
        handle: &DisplayHandle,
        signals: &mut SyncEventQueue<WayVRSignal>,
    ) {
        if self.visible {
            if !self.displayed_windows.is_empty() {
                self.no_windows_since = None;
            } else if let Some(auto_hide_delay) = config.auto_hide_delay {
                if let Some(s) = self.no_windows_since {
                    if s + (auto_hide_delay as u64) < get_millis() {
                        // Auto-hide after specific time
                        signals.send(WayVRSignal::DisplayVisibility(*handle, false));
                    }
                }
            }
        }

        while let Some(task) = self.tasks.read() {
            match task {
                DisplayTask::ProcessCleanup(process_handle) => {
                    let count = self.displayed_windows.len();
                    self.displayed_windows
                        .retain(|win| win.process_handle != process_handle);
                    log::info!(
                        "Cleanup finished for display \"{}\". Current window count: {}",
                        self.name,
                        self.displayed_windows.len()
                    );
                    self.no_windows_since = Some(get_millis());

                    if count != self.displayed_windows.len() {
                        signals.send(WayVRSignal::BroadcastStateChanged(
                            packet_server::WvrStateChanged::WindowRemoved,
                        ));
                    }

                    self.reposition_windows();
                }
            }
        }
    }

    pub fn tick_render(&self, renderer: &mut GlesRenderer, time_ms: u64) -> anyhow::Result<()> {
        renderer.bind(self.gles_texture.clone())?;

        let size = Size::from((self.width as i32, self.height as i32));
        let damage: Rectangle<i32, smithay::utils::Physical> = Rectangle::from_size(size);

        let elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = self
            .displayed_windows
            .iter()
            .flat_map(|display_window| {
                let wm = self.wm.borrow_mut();
                if let Some(window) = wm.windows.get(&display_window.window_handle) {
                    if !window.visible {
                        return vec![];
                    }
                    render_elements_from_surface_tree(
                        renderer,
                        display_window.toplevel.wl_surface(),
                        (window.pos_x, window.pos_y),
                        1.0,
                        1.0,
                        Kind::Unspecified,
                    )
                } else {
                    // Failed to fetch window
                    vec![]
                }
            })
            .collect();

        let mut frame = renderer.render(size, Transform::Normal)?;

        let clear_color = if self.displayed_windows.is_empty() {
            Color32F::new(1.0, 1.0, 1.0, 0.5)
        } else {
            Color32F::new(0.0, 0.0, 0.0, 0.0)
        };

        frame.clear(clear_color, &[damage])?;

        draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;

        let _sync_point = frame.finish()?;

        for window in &self.displayed_windows {
            send_frames_surface_tree(window.toplevel.wl_surface(), time_ms as u32);
        }

        Ok(())
    }

    fn get_hovered_window(&self, cursor_x: u32, cursor_y: u32) -> Option<window::WindowHandle> {
        let wm = self.wm.borrow();

        for cell in self.displayed_windows.iter().rev() {
            if let Some(window) = wm.windows.get(&cell.window_handle) {
                if !window.visible {
                    continue;
                }

                if (cursor_x as i32) >= window.pos_x
                    && (cursor_x as i32) < window.pos_x + window.size_x as i32
                    && (cursor_y as i32) >= window.pos_y
                    && (cursor_y as i32) < window.pos_y + window.size_y as i32
                {
                    return Some(cell.window_handle);
                }
            }
        }
        None
    }

    pub fn trigger_rerender(&mut self) {
        self.wants_redraw = true;
    }

    pub fn set_visible(&mut self, visible: bool) {
        log::info!("Display \"{}\" visible: {}", self.name.as_str(), visible);
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.no_windows_since = None;
            self.trigger_rerender();
        }
    }

    pub fn set_layout(&mut self, layout: packet_server::WvrDisplayWindowLayout) {
        log::info!("Display \"{}\" layout: {:?}", self.name.as_str(), layout);
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        self.trigger_rerender();
        self.reposition_windows();
    }

    pub fn send_mouse_move(
        &self,
        config: &super::Config,
        manager: &mut WayVRCompositor,
        x: u32,
        y: u32,
    ) {
        let current_ms = time::get_millis();
        if self.last_pressed_time_ms + config.click_freeze_time_ms as u64 > current_ms {
            return;
        }

        if let Some(window_handle) = self.get_hovered_window(x, y) {
            let wm = self.wm.borrow();
            if let Some(window) = wm.windows.get(&window_handle) {
                let surf = window.toplevel.wl_surface().clone();
                let point = Point::<f64, Logical>::from((
                    (x as i32 - window.pos_x) as f64,
                    (y as i32 - window.pos_y) as f64,
                ));

                manager.seat_pointer.motion(
                    &mut manager.state,
                    Some((surf, Point::from((0.0, 0.0)))),
                    &input::pointer::MotionEvent {
                        serial: manager.serial_counter.next_serial(),
                        time: 0,
                        location: point,
                    },
                );

                manager.seat_pointer.frame(&mut manager.state);
            }
        }
    }

    fn get_mouse_index_number(index: super::MouseIndex) -> u32 {
        match index {
            super::MouseIndex::Left => 0x110,   /* BTN_LEFT */
            super::MouseIndex::Center => 0x112, /* BTN_MIDDLE */
            super::MouseIndex::Right => 0x111,  /* BTN_RIGHT */
        }
    }

    pub fn send_mouse_down(&mut self, manager: &mut WayVRCompositor, index: super::MouseIndex) {
        // Change keyboard focus to pressed window
        let loc = manager.seat_pointer.current_location();

        self.last_pressed_time_ms = time::get_millis();

        if let Some(window_handle) =
            self.get_hovered_window(loc.x.max(0.0) as u32, loc.y.max(0.0) as u32)
        {
            let wm = self.wm.borrow();
            if let Some(window) = wm.windows.get(&window_handle) {
                let surf = window.toplevel.wl_surface().clone();

                if manager.seat_keyboard.current_focus().is_none() {
                    manager.seat_keyboard.set_focus(
                        &mut manager.state,
                        Some(surf),
                        manager.serial_counter.next_serial(),
                    );
                }
            }
        }

        manager.seat_pointer.button(
            &mut manager.state,
            &input::pointer::ButtonEvent {
                button: Self::get_mouse_index_number(index),
                serial: manager.serial_counter.next_serial(),
                time: 0,
                state: smithay::backend::input::ButtonState::Pressed,
            },
        );

        manager.seat_pointer.frame(&mut manager.state);
    }

    pub fn send_mouse_up(&self, manager: &mut WayVRCompositor, index: super::MouseIndex) {
        manager.seat_pointer.button(
            &mut manager.state,
            &input::pointer::ButtonEvent {
                button: Self::get_mouse_index_number(index),
                serial: manager.serial_counter.next_serial(),
                time: 0,
                state: smithay::backend::input::ButtonState::Released,
            },
        );

        manager.seat_pointer.frame(&mut manager.state);
    }

    pub fn send_mouse_scroll(&self, manager: &mut WayVRCompositor, delta_y: f32, delta_x: f32) {
        manager.seat_pointer.axis(
            &mut manager.state,
            input::pointer::AxisFrame {
                source: None,
                relative_direction: (
                    smithay::backend::input::AxisRelativeDirection::Identical,
                    smithay::backend::input::AxisRelativeDirection::Identical,
                ),
                time: 0,
                axis: (delta_x as f64, -delta_y as f64),
                v120: Some((0, (delta_y * -120.0) as i32)),
                stop: (false, false),
            },
        );
        manager.seat_pointer.frame(&mut manager.state);
    }

    fn configure_env(&self, cmd: &mut std::process::Command, auth_key: &str) {
        cmd.env_remove("DISPLAY"); // Goodbye X11
        cmd.env("WAYLAND_DISPLAY", self.wayland_env.display_num_string());
        cmd.env("WAYVR_DISPLAY_AUTH", auth_key);
    }

    pub fn spawn_process(
        &mut self,
        exec_path: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> anyhow::Result<SpawnProcessResult> {
        log::info!("Spawning subprocess with exec path \"{}\"", exec_path);

        let auth_key = generate_auth_key();

        let mut cmd = std::process::Command::new(exec_path);
        self.configure_env(&mut cmd, auth_key.as_str());
        cmd.args(args);

        for e in env {
            cmd.env(e.0, e.1);
        }

        match cmd.spawn() {
            Ok(child) => Ok(SpawnProcessResult { auth_key, child }),
            Err(e) => {
                anyhow::bail!(
					"Failed to launch process with path \"{}\": {}. Make sure your exec path exists.",
					exec_path,
					e
				);
            }
        }
    }
}

gen_id!(DisplayVec, Display, DisplayCell, DisplayHandle);

impl DisplayHandle {
    pub fn from_packet(handle: packet_server::WvrDisplayHandle) -> Self {
        Self {
            generation: handle.generation,
            idx: handle.idx,
        }
    }

    pub fn as_packet(&self) -> packet_server::WvrDisplayHandle {
        packet_server::WvrDisplayHandle {
            idx: self.idx,
            generation: self.generation,
        }
    }
}
