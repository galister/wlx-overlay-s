use smithay::wayland::shell::xdg::ToplevelSurface;
use wayvr_ipc::packet_server;

use crate::gen_id;

use super::display;

#[derive(Debug)]
pub struct Window {
    pub pos_x: i32,
    pub pos_y: i32,
    pub size_x: u32,
    pub size_y: u32,
    pub visible: bool,
    pub toplevel: ToplevelSurface,
    pub display_handle: display::DisplayHandle,
}

impl Window {
    pub fn new(display_handle: display::DisplayHandle, toplevel: &ToplevelSurface) -> Self {
        Self {
            pos_x: 0,
            pos_y: 0,
            size_x: 0,
            size_y: 0,
            visible: true,
            toplevel: toplevel.clone(),
            display_handle,
        }
    }

    pub fn set_pos(&mut self, pos_x: i32, pos_y: i32) {
        self.pos_x = pos_x;
        self.pos_y = pos_y;
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
}

#[derive(Debug)]
pub struct WindowManager {
    pub windows: WindowVec,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: WindowVec::new(),
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
        display_handle: display::DisplayHandle,
        toplevel: &ToplevelSurface,
    ) -> WindowHandle {
        self.windows.add(Window::new(display_handle, toplevel))
    }
}

gen_id!(WindowVec, Window, WindowCell, WindowHandle);

impl WindowHandle {
    pub fn from_packet(handle: packet_server::WvrWindowHandle) -> Self {
        Self {
            generation: handle.generation,
            idx: handle.idx,
        }
    }

    pub fn as_packet(&self) -> packet_server::WvrWindowHandle {
        packet_server::WvrWindowHandle {
            idx: self.idx,
            generation: self.generation,
        }
    }
}
