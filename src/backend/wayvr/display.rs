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

use crate::gen_id;

use super::{
    client::WayVRManager, comp::send_frames_surface_tree, egl_data, smithay_wrapper, window,
};

fn generate_auth_key() -> String {
    let uuid = uuid::Uuid::new_v4();
    uuid.to_string()
}

struct Process {
    auth_key: String,
    child: std::process::Child,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _dont_care = self.child.kill();
    }
}

struct DisplayWindow {
    handle: window::WindowHandle,
    toplevel: ToplevelSurface,
}

pub struct Display {
    // Display info stuff
    pub width: u32,
    pub height: u32,
    pub name: String,
    wm: Rc<RefCell<window::WindowManager>>,
    displayed_windows: Vec<DisplayWindow>,
    wayland_env: super::WaylandEnv,

    // Render data stuff
    gles_texture: GlesTexture, // TODO: drop texture
    egl_image: khronos_egl::Image,
    egl_data: Rc<egl_data::EGLData>,
    pub dmabuf_data: egl_data::DMAbufData,

    processes: Vec<Process>,
}

impl Drop for Display {
    fn drop(&mut self) {
        let _ = self
            .egl_data
            .egl
            .destroy_image(self.egl_data.display, self.egl_image);
    }
}

impl Display {
    pub fn new(
        wm: Rc<RefCell<window::WindowManager>>,
        renderer: &mut GlesRenderer,
        egl_data: Rc<egl_data::EGLData>,
        wayland_env: super::WaylandEnv,
        width: u32,
        height: u32,
        name: &str,
    ) -> anyhow::Result<Self> {
        let tex_format = ffi::RGBA;
        let internal_format = ffi::RGBA8;

        let tex_id = renderer.with_context(|gl| {
            smithay_wrapper::create_framebuffer_texture(
                gl,
                width,
                height,
                tex_format,
                internal_format,
            )
        })?;

        let egl_image = egl_data.create_egl_image(tex_id, width, height)?;
        let dmabuf_data = egl_data.create_dmabuf_data(&egl_image)?;

        let opaque = false;
        let size = (width as i32, height as i32).into();
        let gles_texture =
            unsafe { GlesTexture::from_raw(renderer, Some(tex_format), opaque, tex_id, size) };

        Ok(Self {
            wm,
            width,
            height,
            name: String::from(name),
            displayed_windows: Vec::new(),
            egl_data,
            dmabuf_data,
            egl_image,
            gles_texture,
            wayland_env,
            processes: Vec::new(),
        })
    }

    pub fn auth_key_matches(&self, auth_key: &str) -> bool {
        for process in &self.processes {
            if process.auth_key.as_str() == auth_key {
                return true;
            }
        }
        false
    }

    pub fn add_window(&mut self, window_handle: window::WindowHandle, toplevel: &ToplevelSurface) {
        log::debug!("Attaching toplevel surface into display");
        self.displayed_windows.push(DisplayWindow {
            handle: window_handle,
            toplevel: toplevel.clone(),
        });
        self.reposition_windows();
    }

    fn reposition_windows(&mut self) {
        let window_count = self.displayed_windows.len();

        for (i, win) in self.displayed_windows.iter_mut().enumerate() {
            if let Some(window) = self.wm.borrow_mut().windows.get_mut(&win.handle) {
                let d_cur = i as f32 / window_count as f32;
                let d_next = (i + 1) as f32 / window_count as f32;

                let left = (d_cur * self.width as f32) as i32;
                let right = (d_next * self.width as f32) as i32;

                window.set_pos(left, 0);
                window.set_size((right - left) as u32, self.height);
            }
        }
    }

    pub fn tick_render(&self, renderer: &mut GlesRenderer, time_ms: u64) -> anyhow::Result<()> {
        renderer.bind(self.gles_texture.clone())?;

        let size = Size::from((self.width as i32, self.height as i32));
        let damage: Rectangle<i32, smithay::utils::Physical> =
            Rectangle::from_loc_and_size((0, 0), size);

        let elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = self
            .displayed_windows
            .iter()
            .flat_map(|display_window| {
                let wm = self.wm.borrow_mut();
                if let Some(window) = wm.windows.get(&display_window.handle) {
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

        let clear_opacity = if self.displayed_windows.is_empty() {
            0.5
        } else {
            0.0
        };

        frame.clear(Color32F::new(1.0, 1.0, 1.0, clear_opacity), &[damage])?;

        draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;

        let _sync_point = frame.finish()?;

        for window in &self.displayed_windows {
            send_frames_surface_tree(window.toplevel.wl_surface(), time_ms as u32);
        }

        Ok(())
    }

    fn get_hovered_window(&self, cursor_x: u32, cursor_y: u32) -> Option<window::WindowHandle> {
        let wm = self.wm.borrow();

        for cell in self.displayed_windows.iter() {
            if let Some(window) = wm.windows.get(&cell.handle) {
                if (cursor_x as i32) >= window.pos_x
                    && (cursor_x as i32) < window.pos_x + window.size_x as i32
                    && (cursor_y as i32) >= window.pos_y
                    && (cursor_y as i32) < window.pos_y + window.size_y as i32
                {
                    return Some(cell.handle);
                }
            }
        }
        None
    }

    pub fn send_mouse_move(&self, manager: &mut WayVRManager, x: u32, y: u32) {
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

    pub fn send_mouse_down(&self, manager: &mut WayVRManager, index: super::MouseIndex) {
        // Change keyboard focus to pressed window
        let loc = manager.seat_pointer.current_location();

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

    pub fn send_mouse_up(&self, manager: &mut WayVRManager, index: super::MouseIndex) {
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

    pub fn send_mouse_scroll(&self, manager: &mut WayVRManager, delta: f32) {
        manager.seat_pointer.axis(
            &mut manager.state,
            input::pointer::AxisFrame {
                source: None,
                relative_direction: (
                    smithay::backend::input::AxisRelativeDirection::Identical,
                    smithay::backend::input::AxisRelativeDirection::Identical,
                ),
                time: 0,
                axis: (0.0, -delta as f64),
                v120: Some((0, (delta * -120.0) as i32)),
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
    ) -> anyhow::Result<()> {
        log::info!("Spawning subprocess with exec path \"{}\"", exec_path);

        let auth_key = generate_auth_key();

        let mut cmd = std::process::Command::new(exec_path);
        self.configure_env(&mut cmd, auth_key.as_str());
        cmd.args(args);

        for e in env {
            cmd.env(e.0, e.1);
        }

        match cmd.spawn() {
            Ok(child) => {
                self.processes.push(Process { child, auth_key });
            }
            Err(e) => {
                anyhow::bail!(
					"Failed to launch process with path \"{}\": {}. Make sure your exec path exists.",
					exec_path,
					e
				);
            }
        }

        Ok(())
    }
}

gen_id!(DisplayVec, Display, DisplayCell, DisplayHandle);
