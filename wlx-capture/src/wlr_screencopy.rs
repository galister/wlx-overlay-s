use libc::{O_CREAT, O_RDWR, S_IRUSR, S_IWUSR};
use std::{
    any::Any,
    ffi::CString,
    os::fd::{BorrowedFd, RawFd},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::JoinHandle,
};
use wayland_client::{
    protocol::{wl_buffer::WlBuffer, wl_shm::Format, wl_shm_pool::WlShmPool},
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};

use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{ZwlrScreencopyFrameV1, self};

use crate::{
    frame::{
        DrmFormat, FourCC, FrameFormat, FramePlane, MemFdFrame, WlxFrame, DRM_FORMAT_ARGB8888,
        DRM_FORMAT_XRGB8888,
    },
    wayland::WlxClient,
    WlxCapture,
};

struct BufData {
    wl_buffer: WlBuffer,
    wl_pool: WlShmPool,
    fd: RawFd,
}

impl Drop for BufData {
    fn drop(&mut self) {
        self.wl_buffer.destroy();
        self.wl_pool.destroy();
        unsafe {
            libc::close(self.fd);
        }
    }
}

enum ScreenCopyEvent {
    Buffer {
        data: BufData,
        fourcc: FourCC,
        width: u32,
        height: u32,
        stride: u32,
    },
    Ready,
    Failed,
}

struct CaptureData<U, R>
where
    U: Any,
    R: Any,
{
    sender: mpsc::SyncSender<R>,
    receiver: mpsc::Receiver<R>,
    user_data: U,
    receive_callback: fn(&U, WlxFrame) -> Option<R>,
}

pub struct WlrScreencopyCapture<U, R>
where
    U: Any + Send,
    R: Any + Send,
{
    output_id: u32,
    wl: Option<Box<WlxClient>>,
    handle: Option<JoinHandle<Box<WlxClient>>>,
    data: Option<CaptureData<U, R>>,
}

impl<U, R> WlrScreencopyCapture<U, R>
where
    U: Any + Send,
    R: Any + Send,
{
    pub fn new(wl: WlxClient, output_id: u32) -> Self {
        Self {
            output_id,
            wl: Some(Box::new(wl)),
            handle: None,
            data: None,
        }
    }
}

impl<U, R> WlxCapture<U, R> for WlrScreencopyCapture<U, R>
where
    U: Any + Send + Clone,
    R: Any + Send,
{
    fn init(
        &mut self,
        _: &[DrmFormat],
        user_data: U,
        receive_callback: fn(&U, WlxFrame) -> Option<R>,
    ) {
        debug_assert!(self.wl.is_some());

        let (sender, receiver) = mpsc::sync_channel(2);
        self.data = Some(CaptureData {
            sender,
            receiver,
            user_data,
            receive_callback,
        });
    }
    fn is_ready(&self) -> bool {
        self.data.is_some()
    }
    fn supports_dmbuf(&self) -> bool {
        false // screencopy v1
    }
    fn receive(&mut self) -> Option<R> {
        if let Some(data) = self.data.as_ref() {
            data.receiver.try_iter().last()
        } else {
            None
        }
    }
    fn pause(&mut self) {}
    fn resume(&mut self) {
        if self.data.is_none() {
            return;
        }
        self.receive(); // clear old frames
        self.request_new_frame();
    }
    fn request_new_frame(&mut self) {
        let mut wait_for_damage = false;
        if let Some(handle) = self.handle.take() {
            if handle.is_finished() {
                wait_for_damage = true;
                self.wl = Some(handle.join().unwrap()); // safe to unwrap because we checked is_finished
            } else {
                self.handle = Some(handle);
                return;
            }
        }

        let Some(wl) = self.wl.take() else {
            return;
        };

        let data = self
            .data
            .as_ref()
            .expect("must call init once before request_new_frame");

        self.handle = Some(std::thread::spawn({
            let sender = data.sender.clone();
            let user_data = data.user_data.clone();
            let receive_callback = data.receive_callback;

            let output_id = self.output_id;
            move || {
                request_screencopy_frame(
                    wl,
                    output_id,
                    sender,
                    user_data,
                    receive_callback,
                    wait_for_damage,
                )
            }
        }));
    }
}

/// Request a new DMA-Buf frame using the wlr-screencopy protocol.
fn request_screencopy_frame<U, R>(
    client: Box<WlxClient>,
    output_id: u32,
    sender: SyncSender<R>,
    user_data: U,
    receive_callback: fn(&U, WlxFrame) -> Option<R>,
    wait_for_damage: bool,
) -> Box<WlxClient>
where
    U: Any + Send,
    R: Any + Send,
{
    let Some(screencopy_manager) = client.maybe_wlr_screencopy_mgr.as_ref() else {
        return client;
    };

    let Some(output) = client.outputs.get(output_id) else {
        return client;
    };

    let transform = output.transform;

    let (tx, rx) = mpsc::sync_channel::<ScreenCopyEvent>(16);

    let proxy =
        screencopy_manager.capture_output(1, &output.wl_output, &client.queue_handle, tx.clone());

    let name = output.name.clone();

    let mut client = client;
    client.dispatch();

    let mut frame_buffer = None;

    'receiver: loop {
        for event in rx.try_iter() {
            match event {
                ScreenCopyEvent::Buffer {
                    data,
                    fourcc,
                    width,
                    height,
                    stride,
                } => {
                    let frame = MemFdFrame {
                        format: FrameFormat {
                            width,
                            height,
                            fourcc,
                            transform,
                            ..Default::default()
                        },
                        plane: FramePlane {
                            fd: Some(data.fd),
                            offset: 0,
                            stride: stride as _,
                        },
                        ..Default::default()
                    };
                    log::trace!("{}: Received screencopy buffer, copying", name.as_ref());
                    if wait_for_damage {
                        proxy.copy_with_damage(&data.wl_buffer);
                    } else {
                        proxy.copy(&data.wl_buffer);
                    }
                    frame_buffer = Some((frame, data));
                    client.dispatch();
                }
                ScreenCopyEvent::Ready => {
                    if let Some((frame, buffer)) = frame_buffer {
                        if let Some(r) = receive_callback(&user_data, WlxFrame::MemFd(frame)) {
                            let _ = sender.send(r);
                            log::trace!("{}: Frame ready", name.as_ref());
                        }
                        drop(buffer);
                    }
                    break 'receiver;
                }
                ScreenCopyEvent::Failed => {
                    log::trace!("{}: Frame failed", name.as_ref());
                    break 'receiver;
                }
            };
        }
    }

    client
}

static FD_COUNTER: AtomicUsize = AtomicUsize::new(0);

impl Dispatch<ZwlrScreencopyFrameV1, SyncSender<ScreenCopyEvent>> for WlxClient {
    fn event(
        state: &mut Self,
        proxy: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        data: &SyncSender<ScreenCopyEvent>,
        _conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Failed => {
                let _ = data.send(ScreenCopyEvent::Failed);
                proxy.destroy();
            }
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                let WEnum::Value(shm_format) = format else {
                    log::warn!("Unknown screencopy format");
                    let _ = data.send(ScreenCopyEvent::Failed);
                    proxy.destroy();
                    return;
                };

                let Some(fourcc) = fourcc_from_wlshm(shm_format) else {
                    log::warn!("Unsupported screencopy format");
                    let _ = data.send(ScreenCopyEvent::Failed);
                    proxy.destroy();
                    return;
                };

                let fd_num = FD_COUNTER.fetch_add(1, Ordering::Relaxed);
                let name = CString::new(format!("wlx-{}", fd_num)).unwrap(); // safe
                let size = stride * height;
                let fd = unsafe {
                    let fd = libc::shm_open(name.as_ptr(), O_CREAT | O_RDWR, S_IRUSR | S_IWUSR);
                    libc::shm_unlink(name.as_ptr());
                    libc::ftruncate(fd, size as _);
                    fd
                };

                let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };

                let wl_pool = state
                    .wl_shm
                    .create_pool(borrowed_fd, size as _, qhandle, ());

                let wl_buffer = wl_pool.create_buffer(
                    0,
                    width as _,
                    height as _,
                    stride as _,
                    shm_format,
                    qhandle,
                    (),
                );

                let _ = data.send(ScreenCopyEvent::Buffer {
                    data: BufData {
                        wl_buffer,
                        wl_pool,
                        fd,
                    },
                    fourcc,
                    width,
                    height,
                    stride,
                });
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                let _ = data.send(ScreenCopyEvent::Ready);
                proxy.destroy();
            }
            _ => {}
        }
    }
}

fn fourcc_from_wlshm(shm_format: Format) -> Option<FourCC> {
    match shm_format {
        Format::Argb8888 => Some(FourCC::from(DRM_FORMAT_ARGB8888)),
        Format::Xrgb8888 => Some(FourCC::from(DRM_FORMAT_XRGB8888)),
        Format::Abgr8888 => Some(FourCC::from(DRM_FORMAT_ARGB8888)),
        Format::Xbgr8888 => Some(FourCC::from(DRM_FORMAT_XRGB8888)),
        _ => None,
    }
}

// Plumbing below

impl Dispatch<WlShmPool, ()> for WlxClient {
    fn event(
        _state: &mut Self,
        _proxy: &WlShmPool,
        _event: <WlShmPool as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for WlxClient {
    fn event(
        _state: &mut Self,
        _proxy: &WlBuffer,
        _event: <WlBuffer as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}
