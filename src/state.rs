use std::{env::VarError, path::Path, sync::Arc};

use glam::{Quat, Vec3};
use vulkano::{command_buffer::CommandBufferUsage, format::Format, image::view::ImageView};

use crate::{
    backend::{common::TaskContainer, input::InputState},
    graphics::WlxGraphics,
    gui::font::FontCache,
    hid::HidProvider,
    shaders::{frag_color, frag_glyph, frag_screen, frag_sprite, frag_srgb, vert_common},
};

pub const WATCH_DEFAULT_POS: Vec3 = Vec3::new(0.025, 0., 0.15);
pub const WATCH_DEFAULT_ROT: Quat = Quat::from_xyzw(0.7071066, 0., 0.7071066, 0.0007963);

pub struct AppState {
    pub fc: FontCache,
    pub session: AppSession,
    pub tasks: TaskContainer,
    pub graphics: Arc<WlxGraphics>,
    pub format: vulkano::format::Format,
    pub input_state: InputState,
    pub hid_provider: Box<dyn HidProvider>,
}

impl AppState {
    pub fn from_graphics(graphics: Arc<WlxGraphics>) -> Self {
        // insert shared resources
        {
            let mut uploads = graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit);
            let texture = uploads.texture2d(1, 1, Format::R8G8B8A8_UNORM, &[255, 0, 255, 255]);
            uploads.build_and_execute_now();

            let Ok(mut images) = graphics.shared_images.write() else {
                panic!("Shared Images RwLock poisoned");
            };

            images.insert("fallback", ImageView::new_default(texture).unwrap());

            let Ok(mut shaders) = graphics.shared_shaders.write() else {
                panic!("Shared Shaders RwLock poisoned");
            };

            let shader = vert_common::load(graphics.device.clone()).unwrap();
            shaders.insert("vert_common", shader);

            let shader = frag_color::load(graphics.device.clone()).unwrap();
            shaders.insert("frag_color", shader);

            let shader = frag_glyph::load(graphics.device.clone()).unwrap();
            shaders.insert("frag_glyph", shader);

            let shader = frag_screen::load(graphics.device.clone()).unwrap();
            shaders.insert("frag_screen", shader);

            let shader = frag_sprite::load(graphics.device.clone()).unwrap();
            shaders.insert("frag_sprite", shader);

            let shader = frag_srgb::load(graphics.device.clone()).unwrap();
            shaders.insert("frag_srgb", shader);
        }

        AppState {
            fc: FontCache::new(),
            session: AppSession::load(),
            tasks: TaskContainer::new(),
            graphics,
            format: Format::R8G8B8A8_UNORM,
            input_state: InputState::new(),
            hid_provider: crate::hid::initialize(),
        }
    }
}

pub struct AppSession {
    pub config_path: String,

    pub show_screens: Vec<String>,
    pub show_keyboard: bool,
    pub keyboard_volume: f32,

    pub screen_flip_h: bool,
    pub screen_flip_v: bool,
    pub screen_invert_color: bool,
    pub screen_max_res: [u32; 2],

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
    pub fn load() -> Self {
        let config_path = std::env::var("XDG_CONFIG_HOME")
            .or_else(|_| std::env::var("HOME").map(|home| format!("{}/.config", home)))
            .or_else(|_| {
                log::warn!("Err: $XDG_CONFIG_HOME and $HOME are not set, using /tmp/wlxoverlay");
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
            screen_max_res: [2560, 1440],
            capture_method: "auto".to_string(),
            primary_hand: 1,
            watch_hand: 0,
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
