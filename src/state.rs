use std::{collections::VecDeque, env::VarError, path::Path, sync::Arc};

use glam::{Quat, Vec3};
use log::warn;

use crate::{
    graphics::WlxGraphics, gui::font::FontCache, input::InputProvider, overlays::OverlayData,
};

pub const WATCH_DEFAULT_POS: Vec3 = Vec3::new(0., 0., 0.15);
pub const WATCH_DEFAULT_ROT: Quat = Quat::from_xyzw(0.7071066, 0., 0.7071066, 0.0007963);

pub type Task = Box<dyn FnOnce(&mut AppState, &mut [OverlayData]) + Send>;

pub struct AppState {
    pub fc: FontCache,
    //pub input: InputState,
    pub session: AppSession,
    pub tasks: VecDeque<Task>,
    pub graphics: Arc<WlxGraphics>,
    pub format: vulkano::format::Format,
    pub input: Box<dyn InputProvider>,
}

pub struct AppSession {
    pub config_path: String,

    pub show_screens: Vec<String>,
    pub show_keyboard: bool,
    pub keyboard_volume: f32,

    pub screen_flip_h: bool,
    pub screen_flip_v: bool,
    pub screen_invert_color: bool,

    pub watch_hand: usize,
    pub watch_pos: Vec3,
    pub watch_rot: Quat,

    pub primary_hand: usize,

    pub capture_method: String,

    pub color_norm: Vec3,
    pub color_shift: Vec3,
    pub color_alt: Vec3,
    pub color_grab: Vec3,

    pub click_freeze_time_ms: u64,
}

impl AppSession {
    pub fn load() -> AppSession {
        let config_path = std::env::var("XDG_CONFIG_HOME")
            .or_else(|_| std::env::var("HOME").map(|home| format!("{}/.config", home)))
            .or_else(|_| {
                warn!("Err: $XDG_CONFIG_HOME and $HOME are not set, using /tmp/wlxoverlay");
                Ok::<String, VarError>("/tmp".to_string())
            })
            .map(|config| Path::new(&config).join("wlxoverlay"))
            .ok()
            .and_then(|path| path.to_str().map(|path| path.to_string()))
            .unwrap();

        let _ = std::fs::create_dir(&config_path);

        AppSession {
            config_path,
            show_screens: vec!["DP-3".to_string()],
            keyboard_volume: 0.5,
            show_keyboard: false,
            screen_flip_h: false,
            screen_flip_v: false,
            screen_invert_color: false,
            capture_method: "auto".to_string(),
            primary_hand: 1,
            watch_hand: 1,
            watch_pos: WATCH_DEFAULT_POS,
            watch_rot: WATCH_DEFAULT_ROT,
            color_norm: Vec3 {
                x: 0.,
                y: 1.,
                z: 1.,
            },
            color_shift: Vec3 {
                x: 1.,
                y: 1.,
                z: 0.,
            },
            color_alt: Vec3 {
                x: 1.,
                y: 0.,
                z: 1.,
            },
            color_grab: Vec3 {
                x: 1.,
                y: 0.,
                z: 0.,
            },
            click_freeze_time_ms: 300,
        }
    }
}
