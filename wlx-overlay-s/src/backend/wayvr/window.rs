use smithay::{
    input,
    utils::{Logical, Point},
    wayland::shell::xdg::ToplevelSurface,
};
use wayvr_ipc::packet_server;

use crate::{
    backend::wayvr::{client::WayVRCompositor, process},
    gen_id,
    subsystem::hid::WheelDelta,
};

#[derive(Debug)]
pub struct Window {
    pub size_x: u32,
    pub size_y: u32,
    pub visible: bool,
    pub toplevel: ToplevelSurface,
    pub process: process::ProcessHandle,
}

impl Window {
    fn new(toplevel: &ToplevelSurface, process: process::ProcessHandle) -> Self {
        Self {
            size_x: 0,
            size_y: 0,
            visible: true,
            toplevel: toplevel.clone(),
            process,
        }
    }

    pub fn set_size(&mut self, size_x: u32, size_y: u32) {
        self.toplevel.with_pending_state(|state| {
            //state.bounds = Some((size_x as i32, size_y as i32).into());
            state.size = Some((size_x as i32, size_y as i32).into());
        });
        self.toplevel.send_configure();

        self.size_x = size_x;
        self.size_y = size_y;
    }

    pub(super) fn send_mouse_move(&self, manager: &mut WayVRCompositor, x: u32, y: u32) {
        let surf = self.toplevel.wl_surface().clone();
        let point = Point::<f64, Logical>::from((f64::from(x as i32), f64::from(y as i32)));

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

    const fn get_mouse_index_number(index: super::MouseIndex) -> u32 {
        match index {
            super::MouseIndex::Left => 0x110,   /* BTN_LEFT */
            super::MouseIndex::Center => 0x112, /* BTN_MIDDLE */
            super::MouseIndex::Right => 0x111,  /* BTN_RIGHT */
        }
    }

    pub(super) fn send_mouse_down(
        &mut self,
        manager: &mut WayVRCompositor,
        index: super::MouseIndex,
    ) {
        let surf = self.toplevel.wl_surface().clone();

        // Change keyboard focus to pressed window
        manager.seat_keyboard.set_focus(
            &mut manager.state,
            Some(surf),
            manager.serial_counter.next_serial(),
        );

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

    pub(super) fn send_mouse_up(manager: &mut WayVRCompositor, index: super::MouseIndex) {
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

    pub(super) fn send_mouse_scroll(manager: &mut WayVRCompositor, delta: WheelDelta) {
        manager.seat_pointer.axis(
            &mut manager.state,
            input::pointer::AxisFrame {
                source: None,
                relative_direction: (
                    smithay::backend::input::AxisRelativeDirection::Identical,
                    smithay::backend::input::AxisRelativeDirection::Identical,
                ),
                time: 0,
                axis: (f64::from(delta.x), f64::from(-delta.y)),
                v120: Some((0, (delta.y * -64.0) as i32)),
                stop: (false, false),
            },
        );
        manager.seat_pointer.frame(&mut manager.state);
    }
}

#[derive(Debug)]
pub struct MouseState {
    pub hover_window: WindowHandle,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug)]
pub struct WindowManager {
    pub windows: WindowVec,
    pub mouse: Option<MouseState>,
}

impl WindowManager {
    pub const fn new() -> Self {
        Self {
            windows: WindowVec::new(),
            mouse: None,
        }
    }

    pub fn find_window_handle(&self, toplevel: &ToplevelSurface) -> Option<WindowHandle> {
        for (idx, cell) in self.windows.vec.iter().enumerate() {
            if let Some(cell) = cell {
                let window = &cell.obj;
                if window.toplevel == *toplevel {
                    return Some(WindowVec::get_handle(cell, idx));
                }
            }
        }
        None
    }

    pub fn create_window(
        &mut self,
        toplevel: &ToplevelSurface,
        process: process::ProcessHandle,
    ) -> WindowHandle {
        self.windows.add(Window::new(toplevel, process))
    }

    pub fn remove_window(&mut self, window_handle: WindowHandle) {
        self.windows.remove(&window_handle);
    }
}

gen_id!(WindowVec, Window, WindowCell, WindowHandle);

impl WindowHandle {
    pub const fn from_packet(handle: packet_server::WvrWindowHandle) -> Self {
        Self {
            generation: handle.generation,
            idx: handle.idx,
        }
    }

    pub const fn as_packet(&self) -> packet_server::WvrWindowHandle {
        packet_server::WvrWindowHandle {
            idx: self.idx,
            generation: self.generation,
        }
    }
}
