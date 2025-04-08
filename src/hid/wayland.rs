use wlx_capture::wayland::wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{
        wl_keyboard::{self, WlKeyboard},
        wl_registry::WlRegistry,
        wl_seat::{self, Capability, WlSeat},
    },
    Connection, Dispatch, Proxy, QueueHandle,
};
use xkbcommon::xkb;

use super::XkbKeymap;

struct WlKeymapHandler {
    seat: WlSeat,
    keyboard: Option<WlKeyboard>,
    keymap: Option<XkbKeymap>,
}

impl Drop for WlKeymapHandler {
    fn drop(&mut self) {
        if let Some(keyboard) = &self.keyboard {
            keyboard.release();
        }
        self.seat.release();
    }
}

pub fn get_keymap_wl() -> anyhow::Result<XkbKeymap> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<WlKeymapHandler>(&connection)?;
    let qh = queue.handle();
    let seat: WlSeat = globals
        .bind(&qh, 4..=9, ())
        .expect(WlSeat::interface().name);

    let mut me = WlKeymapHandler {
        seat,
        keyboard: None,
        keymap: None,
    };

    // this gets us the wl_seat
    let _ = queue.blocking_dispatch(&mut me);

    // this gets us the wl_keyboard
    let _ = queue.blocking_dispatch(&mut me);

    me.keymap
        .take()
        .ok_or_else(|| anyhow::anyhow!("Could not load keymap"))
}

impl Dispatch<WlRegistry, GlobalListContents> for WlKeymapHandler {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for WlKeymapHandler {
    fn event(
        state: &mut Self,
        proxy: &WlSeat,
        event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Capabilities { capabilities } => {
                let capability = capabilities
                    .into_result()
                    .unwrap_or(wl_seat::Capability::empty());
                if capability.contains(Capability::Keyboard) {
                    state.keyboard = Some(proxy.get_keyboard(qhandle, ()));
                }
            }
            wl_seat::Event::Name { name } => {
                log::debug!("Using WlSeat: {name}");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for WlKeymapHandler {
    fn event(
        state: &mut Self,
        _proxy: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                let format = format
                    .into_result()
                    .unwrap_or(wl_keyboard::KeymapFormat::NoKeymap);

                if matches!(format, wl_keyboard::KeymapFormat::XkbV1) {
                    let context = xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES);
                    let maybe_keymap = unsafe {
                        xkb::Keymap::new_from_fd(
                            &context,
                            fd,
                            size as _,
                            xkb::KEYMAP_FORMAT_TEXT_V1,
                            xkb::KEYMAP_COMPILE_NO_FLAGS,
                        )
                    };

                    match maybe_keymap {
                        Ok(Some(keymap)) => {
                            state.keymap = Some(XkbKeymap { keymap });
                        }
                        Ok(None) => {
                            log::error!("Could not load keymap: no keymap");
                            log::error!("Default layout will be used.");
                        }
                        Err(err) => {
                            log::error!("Could not load keymap: {err}");
                            log::error!("Default layout will be used.");
                        }
                    }
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                log::debug!("WlKeyboard RepeatInfo rate: {rate}, delay: {delay}");
            }
            _ => {}
        }
    }
}
