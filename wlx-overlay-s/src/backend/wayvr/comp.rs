use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::{BufferType, buffer_type};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_output, wl_seat, wl_surface};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::dmabuf::{
    DmabufFeedback, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf,
};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::shm::{ShmHandler, ShmState, with_buffer_contents};
use smithay::wayland::single_pixel_buffer::get_single_pixel_buffer;
use smithay::{
    delegate_compositor, delegate_data_device, delegate_dmabuf, delegate_output, delegate_seat,
    delegate_shm, delegate_xdg_shell,
};
use std::collections::HashSet;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};

use smithay::utils::Serial;
use smithay::wayland::compositor::{
    self, BufferAssignment, SurfaceAttributes, TraversalAction, with_surface_tree_downward,
};

use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use wayland_server::Client;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::backend::wayvr::SurfaceBufWithImage;
use crate::backend::wayvr::image_importer::ImageImporter;
use crate::ipc::event_queue::SyncEventQueue;

use super::WayVRTask;

pub struct Application {
    pub image_importer: ImageImporter,
    pub dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    pub compositor: compositor::CompositorState,
    pub xdg_shell: XdgShellState,
    pub seat_state: SeatState<Application>,
    pub shm: ShmState,
    pub data_device: DataDeviceState,
    pub wayvr_tasks: SyncEventQueue<WayVRTask>,
    pub redraw_requests: HashSet<wayland_server::backend::ObjectId>,
}

impl Application {
    pub fn check_redraw(&mut self, surface: &WlSurface) -> bool {
        self.redraw_requests.remove(&surface.id())
    }

    pub fn cleanup(&mut self) {
        self.image_importer.cleanup();
    }
}

impl compositor::CompositorHandler for Application {
    fn compositor_state(&mut self) -> &mut compositor::CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a Client,
    ) -> &'a compositor::CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        smithay::wayland::compositor::with_states(surface, |states| {
            let mut guard = states.cached_state.get::<SurfaceAttributes>();
            let attrs = guard.current();

            match attrs.buffer.take() {
                Some(BufferAssignment::NewBuffer(buffer)) => {
                    let current = SurfaceBufWithImage::get_from_surface(states);

                    if current.is_none_or(|c| c.buffer != buffer) {
                        match buffer_type(&buffer) {
                            Some(BufferType::Dma) => {
                                let dmabuf = get_dmabuf(&buffer).unwrap(); // always Ok due to buffer_type
                                if let Ok(image) =
                                    self.image_importer.get_or_import_dmabuf(dmabuf.clone())
                                {
                                    let sbwi = SurfaceBufWithImage {
                                        image,
                                        buffer,
                                        transform: wl_transform_to_frame_transform(
                                            attrs.buffer_transform,
                                        ),
                                        scale: attrs.buffer_scale,
                                    };
                                    sbwi.apply_to_surface(states);
                                }
                            }
                            Some(BufferType::Shm) => {
                                with_buffer_contents(&buffer, |data, size, buf| {
                                    if let Ok(image) =
                                        self.image_importer.import_shm(data, size, buf)
                                    {
                                        let sbwi = SurfaceBufWithImage {
                                            image,
                                            buffer: buffer.clone(),
                                            transform: wl_transform_to_frame_transform(
                                                attrs.buffer_transform,
                                            ),
                                            scale: attrs.buffer_scale,
                                        };
                                        sbwi.apply_to_surface(states);
                                    }
                                });
                            }
                            Some(BufferType::SinglePixel) => {
                                let spb = get_single_pixel_buffer(&buffer).unwrap(); // always Ok
                                if let Ok(image) = self.image_importer.import_spb(spb) {
                                    let sbwi = SurfaceBufWithImage {
                                        image,
                                        buffer,
                                        transform: wl_transform_to_frame_transform(
                                            // does this even matter
                                            attrs.buffer_transform,
                                        ),
                                        scale: attrs.buffer_scale,
                                    };
                                    sbwi.apply_to_surface(states);
                                }
                            }
                            Some(other) => log::warn!("Unsupported wl_buffer format: {other:?}"),
                            None => { /* don't draw anything */ }
                        }
                    }
                }
                Some(BufferAssignment::Removed) => {}
                None => {}
            }
        });

        self.redraw_requests.insert(surface.id());
    }
}

impl SeatHandler for Application {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
}

impl BufferHandler for Application {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ClientDndGrabHandler for Application {}

impl ServerDndGrabHandler for Application {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl DataDeviceHandler for Application {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device
    }
}

impl SelectionHandler for Application {
    type SelectionUserData = ();
}

#[derive(Default)]
pub struct ClientState {
    compositor_state: compositor::CompositorClientState,
    pub disconnected: Arc<Mutex<bool>>,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        log::debug!("Client ID {client_id:?} connected");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        *self.disconnected.lock().unwrap() = true;
        log::debug!("Client ID {client_id:?} disconnected. Reason: {reason:?}");
    }
}

impl AsMut<compositor::CompositorState> for Application {
    fn as_mut(&mut self) -> &mut compositor::CompositorState {
        &mut self.compositor
    }
}

impl XdgShellHandler for Application {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        if let Some(client) = surface.wl_surface().client() {
            self.wayvr_tasks
                .send(WayVRTask::NewToplevel(client.id(), surface.clone()));
        }
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(client) = surface.wl_surface().client() {
            self.wayvr_tasks
                .send(WayVRTask::DropToplevel(client.id(), surface.clone()));
        }
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {
        // Handle popup creation here
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Handle popup grab here
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
        // Handle popup reposition here
    }
}

impl ShmHandler for Application {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl OutputHandler for Application {}

impl DmabufHandler for Application {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state.0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self.image_importer.get_or_import_dmabuf(dmabuf).is_ok() {
            let _ = notifier.successful::<Self>();
        } else {
            notifier.failed();
        }
    }
}

delegate_dmabuf!(Application);
delegate_xdg_shell!(Application);
delegate_compositor!(Application);
delegate_shm!(Application);
delegate_seat!(Application);
delegate_data_device!(Application);
delegate_output!(Application);

pub fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            // the surface may not have any user_data if it is a subsurface and has not
            // yet been committed
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

fn wl_transform_to_frame_transform(
    transform: wl_output::Transform,
) -> wlx_capture::frame::Transform {
    match transform {
        wl_output::Transform::Normal => wlx_capture::frame::Transform::Normal,
        wl_output::Transform::_90 => wlx_capture::frame::Transform::Rotated90,
        wl_output::Transform::_180 => wlx_capture::frame::Transform::Rotated180,
        wl_output::Transform::_270 => wlx_capture::frame::Transform::Rotated270,
        wl_output::Transform::Flipped => wlx_capture::frame::Transform::Flipped,
        wl_output::Transform::Flipped90 => wlx_capture::frame::Transform::Flipped90,
        wl_output::Transform::Flipped180 => wlx_capture::frame::Transform::Flipped180,
        wl_output::Transform::Flipped270 => wlx_capture::frame::Transform::Flipped270,
        _ => wlx_capture::frame::Transform::Undefined,
    }
}
