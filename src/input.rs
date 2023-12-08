use glam::Vec2;
use idmap::{idmap, IdMap};
use idmap_derive::IntegerId;
use input_linux::{
    AbsoluteAxis, AbsoluteInfo, AbsoluteInfoSetup, EventKind, InputId, Key, RelativeAxis,
    UInputHandle,
};
use libc::{input_event, timeval};
use once_cell::sync::Lazy;
use std::fs::File;
use std::mem::transmute;
use strum::{EnumIter, EnumString, IntoEnumIterator};

pub fn initialize_input() -> Box<dyn InputProvider> {
    if let Some(uinput) = UInputProvider::try_new() {
        log::info!("Initialized uinput.");
        return Box::new(uinput);
    }
    log::error!("Could not create uinput provider. Keyboard/Mouse input will not work!");
    log::error!("Check if you're in `input` group: `id -nG`");
    Box::new(DummyProvider {})
}

pub trait InputProvider {
    fn mouse_move(&mut self, pos: Vec2);
    fn send_button(&self, button: u16, down: bool);
    fn wheel(&self, delta: i32);
    fn set_modifiers(&mut self, mods: u8);
    fn send_key(&self, key: u16, down: bool);
    fn set_desktop_extent(&mut self, extent: Vec2);
    fn on_new_frame(&mut self);
}

pub struct UInputProvider {
    handle: UInputHandle<File>,
    desktop_extent: Vec2,
    mouse_moved: bool,
    cur_modifiers: u8,
}

pub struct DummyProvider;

pub const MOUSE_LEFT: u16 = 0x110;
pub const MOUSE_RIGHT: u16 = 0x111;
pub const MOUSE_MIDDLE: u16 = 0x112;

const MOUSE_EXTENT: f32 = 32768.;

const EV_SYN: u16 = 0x0;
const EV_KEY: u16 = 0x1;
const EV_REL: u16 = 0x2;
const EV_ABS: u16 = 0x3;

impl UInputProvider {
    fn try_new() -> Option<Self> {
        if let Ok(file) = File::create("/dev/uinput") {
            let handle = UInputHandle::new(file);

            let id = InputId {
                bustype: 0x03,
                vendor: 0x4711,
                product: 0x0829,
                version: 5,
            };

            let name = b"WlxOverlay-S Keyboard-Mouse Hybrid Thing\0";

            let abs_info = vec![
                AbsoluteInfoSetup {
                    axis: input_linux::AbsoluteAxis::X,
                    info: AbsoluteInfo {
                        value: 0,
                        minimum: 0,
                        maximum: MOUSE_EXTENT as _,
                        fuzz: 0,
                        flat: 0,
                        resolution: 10,
                    },
                },
                AbsoluteInfoSetup {
                    axis: input_linux::AbsoluteAxis::Y,
                    info: AbsoluteInfo {
                        value: 0,
                        minimum: 0,
                        maximum: MOUSE_EXTENT as _,
                        fuzz: 0,
                        flat: 0,
                        resolution: 10,
                    },
                },
            ];

            if handle.set_evbit(EventKind::Key).is_err() {
                return None;
            }
            if handle.set_evbit(EventKind::Absolute).is_err() {
                return None;
            }
            if handle.set_evbit(EventKind::Relative).is_err() {
                return None;
            }

            for btn in MOUSE_LEFT..=MOUSE_MIDDLE {
                let key: Key = unsafe { transmute(btn) };
                if handle.set_keybit(key).is_err() {
                    return None;
                }
            }

            for key in VirtualKey::iter() {
                let key: Key = unsafe { transmute(key as u16) };
                if handle.set_keybit(key).is_err() {
                    return None;
                }
            }

            if handle.set_absbit(AbsoluteAxis::X).is_err() {
                return None;
            }
            if handle.set_absbit(AbsoluteAxis::Y).is_err() {
                return None;
            }
            if handle.set_relbit(RelativeAxis::Wheel).is_err() {
                return None;
            }

            if handle.create(&id, name, 0, &abs_info).is_ok() {
                return Some(UInputProvider {
                    handle,
                    desktop_extent: Vec2::ZERO,
                    mouse_moved: false,
                    cur_modifiers: 0,
                });
            }
        }
        None
    }
}

impl InputProvider for UInputProvider {
    fn mouse_move(&mut self, pos: Vec2) {
        if self.mouse_moved {
            return;
        }
        self.mouse_moved = true;

        #[cfg(debug_assertions)]
        log::trace!("Mouse move: {:?}", pos);

        let pos = pos * (MOUSE_EXTENT / self.desktop_extent);

        let time = get_time();
        let events = [
            new_event(time, EV_ABS, AbsoluteAxis::X as _, pos.x as i32),
            new_event(time, EV_ABS, AbsoluteAxis::Y as _, pos.y as i32),
            new_event(time, EV_SYN, 0, 0),
        ];
        if let Err(res) = self.handle.write(&events) {
            log::error!("{}", res.to_string());
        }
    }
    fn send_button(&self, button: u16, down: bool) {
        let time = get_time();
        let events = [
            new_event(time, EV_KEY, button, down as _),
            new_event(time, EV_SYN, 0, 0),
        ];
        if let Err(res) = self.handle.write(&events) {
            log::error!("send_button: {}", res.to_string());
        }
    }
    fn wheel(&self, delta: i32) {
        let time = get_time();
        let events = [
            new_event(time, EV_REL, RelativeAxis::Wheel as _, delta),
            new_event(time, EV_SYN, 0, 0),
        ];
        if let Err(res) = self.handle.write(&events) {
            log::error!("wheel: {}", res.to_string());
        }
    }
    fn set_modifiers(&mut self, modifiers: u8) {
        let changed = self.cur_modifiers ^ modifiers;
        for i in 0..7 {
            let m = 1 << i;
            if changed & m != 0 {
                let vk = MODS_TO_KEYS.get(m).unwrap()[0] as u16;
                self.send_key(vk, modifiers & m != 0);
            }
        }
        self.cur_modifiers = modifiers;
    }
    fn send_key(&self, key: u16, down: bool) {
        let time = get_time();
        let events = [
            new_event(time, EV_KEY, key - 8, down as _),
            new_event(time, EV_SYN, 0, 0),
        ];
        if let Err(res) = self.handle.write(&events) {
            log::error!("send_key: {}", res.to_string());
        }
    }
    fn set_desktop_extent(&mut self, extent: Vec2) {
        log::info!("Desktop extent: {:?}", extent);
        self.desktop_extent = extent;
    }
    fn on_new_frame(&mut self) {
        self.mouse_moved = false;
    }
}

impl InputProvider for DummyProvider {
    fn mouse_move(&mut self, _pos: Vec2) {}
    fn send_button(&self, _button: u16, _down: bool) {}
    fn wheel(&self, _delta: i32) {}
    fn set_modifiers(&mut self, _modifiers: u8) {}
    fn send_key(&self, _key: u16, _down: bool) {}
    fn set_desktop_extent(&mut self, _extent: Vec2) {}
    fn on_new_frame(&mut self) {}
}

#[inline]
fn get_time() -> timeval {
    let mut time = timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe { libc::gettimeofday(&mut time, std::ptr::null_mut()) };
    time
}

#[inline]
fn new_event(time: timeval, type_: u16, code: u16, value: i32) -> input_event {
    input_event {
        time,
        type_,
        code,
        value,
    }
}

pub type KeyModifier = u8;
pub const SHIFT: KeyModifier = 0x01;
pub const CAPS_LOCK: KeyModifier = 0x02;
pub const CTRL: KeyModifier = 0x04;
pub const ALT: KeyModifier = 0x08;
pub const NUM_LOCK: KeyModifier = 0x10;
pub const SUPER: KeyModifier = 0x40;
pub const META: KeyModifier = 0x80;

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, Clone, Copy, IntegerId, EnumString, EnumIter)]
pub enum VirtualKey {
    Escape = 9,
    N1, // number row
    N2,
    N3,
    N4,
    N5,
    N6,
    N7,
    N8,
    N9,
    N0,
    Minus,
    Plus,
    BackSpace,
    Tab,
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    Oem4, // [ {
    Oem6, // ] }
    Return,
    LCtrl,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    Oem1, // ; :
    Oem7, // ' "
    Oem3, // ` ~
    LShift,
    Oem5, // \ |
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
    Comma,  // , <
    Period, // . >
    Oem2,   // / ?
    RShift,
    KP_Multiply,
    LAlt,
    Space,
    Caps,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    NumLock,
    Scroll,
    KP_7, // KeyPad
    KP_8,
    KP_9,
    KP_Subtract,
    KP_4,
    KP_5,
    KP_6,
    KP_Add,
    KP_1,
    KP_2,
    KP_3,
    KP_0,
    KP_Decimal,
    Oem102 = 94, // Optional key usually between LShift and Z
    F11,
    F12,
    AbntC1,
    Katakana,
    Hiragana,
    Henkan,
    Kana,
    Muhenkan,
    KP_Enter = 104,
    RCtrl,
    KP_Divide,
    Print,
    Meta, // Right Alt aka AltGr
    Home = 110,
    Up,
    Prior,
    Left,
    Right,
    End,
    Down,
    Next,
    Insert,
    Delete,
    XF86AudioMute = 121,
    XF86AudioLowerVolume,
    XF86AudioRaiseVolume,
    Pause = 127,
    AbntC2 = 129,
    Hangul,
    Hanja,
    LSuper = 133,
    RSuper,
    Menu,
    Help = 146,
    XF86MenuKB,
    XF86Sleep = 150,
    XF86Xfer = 155,
    XF86Launch1,
    XF86Launch2,
    XF86WWW,
    XF86Mail = 163,
    XF86Favorites,
    XF86MyComputer,
    XF86Back,
    XF86Forward,
    XF86AudioNext = 171,
    XF86AudioPlay,
    XF86AudioPrev,
    XF86AudioStop,
    XF86HomePage = 180,
    XF86Reload,
    F13 = 191,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Hyper = 207,
    XF86Launch3,
    XF86Launch4,
    XF86LaunchB,
    XF86Search = 225,
}

pub static KEYS_TO_MODS: Lazy<IdMap<VirtualKey, KeyModifier>> = Lazy::new(|| {
    idmap! {
        VirtualKey::LShift => SHIFT,
        VirtualKey::RShift => SHIFT,
        VirtualKey::Caps => CAPS_LOCK,
        VirtualKey::LCtrl => CTRL,
        VirtualKey::RCtrl => CTRL,
        VirtualKey::LAlt => ALT,
        VirtualKey::NumLock => NUM_LOCK,
        VirtualKey::LSuper => SUPER,
        VirtualKey::RSuper => SUPER,
        VirtualKey::Meta => META,
    }
});

pub static MODS_TO_KEYS: Lazy<IdMap<KeyModifier, Vec<VirtualKey>>> = Lazy::new(|| {
    idmap! {
        SHIFT => vec![VirtualKey::LShift, VirtualKey::RShift],
        CAPS_LOCK => vec![VirtualKey::Caps],
        CTRL => vec![VirtualKey::LCtrl, VirtualKey::RCtrl],
        ALT => vec![VirtualKey::LAlt],
        NUM_LOCK => vec![VirtualKey::NumLock],
        SUPER => vec![VirtualKey::LSuper, VirtualKey::RSuper],
        META => vec![VirtualKey::Meta],
    }
});
