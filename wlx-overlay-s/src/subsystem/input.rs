use super::hid::{self, HidProvider, VirtualKey};

#[cfg(feature = "wayvr")]
use crate::overlays::wayvr::WayVRData;
use crate::subsystem::hid::XkbKeymap;
#[cfg(feature = "wayvr")]
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyboardFocus {
    PhysicalScreen,

    #[allow(dead_code)] // Not available if "wayvr" feature is disabled
    WayVR, // (for now without wayland window id data, it's handled internally),
}

pub struct HidWrapper {
    pub keyboard_focus: KeyboardFocus,
    pub inner: Box<dyn HidProvider>,
    pub keymap: Option<XkbKeymap>,
    #[cfg(feature = "wayvr")]
    pub wayvr: Option<Rc<RefCell<WayVRData>>>, // Dynamically created if requested
}

impl HidWrapper {
    pub fn new() -> Self {
        Self {
            keyboard_focus: KeyboardFocus::PhysicalScreen,
            inner: hid::initialize(),
            #[cfg(feature = "wayvr")]
            wayvr: None,
            keymap: None,
        }
    }

    #[cfg(feature = "wayvr")]
    pub fn set_wayvr(&mut self, wayvr: Rc<RefCell<WayVRData>>) {
        if let Some(keymap) = self.keymap.take() {
            let _ = wayvr
                .borrow_mut()
                .data
                .state
                .set_keymap(&keymap.inner)
                .inspect_err(|e| log::error!("Could not set WayVR keymap: {e:?}"));
        }
        self.wayvr = Some(wayvr);
    }

    pub fn send_key_routed(&self, key: VirtualKey, down: bool) {
        match self.keyboard_focus {
            KeyboardFocus::PhysicalScreen => self.inner.send_key(key, down),
            KeyboardFocus::WayVR =>
            {
                #[cfg(feature = "wayvr")]
                if let Some(wayvr) = &self.wayvr {
                    wayvr.borrow_mut().data.state.send_key(key as u32, down);
                }
            }
        }
    }

    pub fn keymap_changed(&mut self, keymap: &XkbKeymap) {
        #[cfg(feature = "wayvr")]
        if let Some(wayvr) = &self.wayvr {
            let _ = wayvr
                .borrow_mut()
                .data
                .state
                .set_keymap(&keymap.inner)
                .inspect_err(|e| log::error!("Could not set WayVR keymap: {e:?}"));
        } else {
            self.keymap = Some(keymap.clone());
        }

        log::info!(
            "Keymap changed: {}",
            keymap.inner.layouts().next().unwrap_or("Unknown")
        );
    }

    pub fn set_modifiers_routed(&mut self, mods: u8) {
        match self.keyboard_focus {
            KeyboardFocus::PhysicalScreen => self.inner.set_modifiers(mods),
            KeyboardFocus::WayVR =>
            {
                #[cfg(feature = "wayvr")]
                if let Some(wayvr) = &self.wayvr {
                    wayvr.borrow_mut().data.state.set_modifiers(mods);
                }
            }
        }
    }
}
