use xkbcommon::xkb::{
    self,
    x11::{get_core_keyboard_device_id, keymap_new_from_device},
};

use super::XkbKeymap;

pub fn get_keymap_x11() -> anyhow::Result<XkbKeymap> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);

    let (conn, _) = xcb::Connection::connect(None)?;
    let device_id = get_core_keyboard_device_id(&conn);
    if device_id == -1 {
        return Err(anyhow::anyhow!(
            "get_core_keyboard_device_id returned -1. Check your XKB installation."
        ));
    }
    let keymap = keymap_new_from_device(&context, &conn, device_id, xkb::KEYMAP_COMPILE_NO_FLAGS);

    Ok(XkbKeymap { context, keymap })
}
