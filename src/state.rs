use std::{path::PathBuf, sync::Arc};

use anyhow::bail;
use glam::{Quat, Vec3};
use rodio::{OutputStream, OutputStreamHandle};
use vulkano::format::Format;

use crate::{
    backend::{common::TaskContainer, input::InputState},
    config::GeneralConfig,
    config_io,
    graphics::WlxGraphics,
    gui::font::FontCache,
    hid::HidProvider,
    shaders::{frag_color, frag_glyph, frag_sprite, frag_srgb, vert_common},
};

pub const WATCH_DEFAULT_POS: Vec3 = Vec3::new(-0.03, -0.01, 0.125);
pub const WATCH_DEFAULT_ROT: Quat = Quat::from_xyzw(-0.7071066, 0.0007963618, 0.7071066, 0.0);

pub struct AppState {
    pub fc: FontCache,
    pub session: AppSession,
    pub tasks: TaskContainer,
    pub graphics: Arc<WlxGraphics>,
    pub format: vulkano::format::Format,
    pub input_state: InputState,
    pub hid_provider: Box<dyn HidProvider>,
    pub audio: AudioOutput,
}

impl AppState {
    pub fn from_graphics(graphics: Arc<WlxGraphics>) -> anyhow::Result<Self> {
        // insert shared resources
        {
            let Ok(mut shaders) = graphics.shared_shaders.write() else {
                bail!("Failed to lock shared shaders");
            };

            let shader = vert_common::load(graphics.device.clone())?;
            shaders.insert("vert_common", shader);

            let shader = frag_color::load(graphics.device.clone())?;
            shaders.insert("frag_color", shader);

            let shader = frag_glyph::load(graphics.device.clone())?;
            shaders.insert("frag_glyph", shader);

            let shader = frag_sprite::load(graphics.device.clone())?;
            shaders.insert("frag_sprite", shader);

            let shader = frag_srgb::load(graphics.device.clone())?;
            shaders.insert("frag_srgb", shader);
        }

        Ok(AppState {
            fc: FontCache::new()?,
            session: AppSession::load(),
            tasks: TaskContainer::new(),
            graphics,
            format: Format::R8G8B8A8_UNORM,
            input_state: InputState::new(),
            hid_provider: crate::hid::initialize(),
            audio: AudioOutput::new(),
        })
    }
}

pub struct AppSession {
    pub config_root_path: PathBuf,
    pub config: GeneralConfig,

    pub watch_hand: usize,
    pub watch_pos: Vec3,
    pub watch_rot: Quat,

    pub primary_hand: usize,

    pub color_norm: Vec3,
    pub color_shift: Vec3,
    pub color_alt: Vec3,
    pub color_grab: Vec3,
}

impl AppSession {
    pub fn load() -> Self {
        let config_root_path = config_io::ensure_config_root();
        log::info!("Config root path: {}", config_root_path.to_string_lossy());
        let config = GeneralConfig::load_from_disk();

        AppSession {
            config_root_path,
            config,
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
        }
    }
}

pub struct AudioOutput {
    audio_stream: Option<(OutputStream, OutputStreamHandle)>,
    first_try: bool,
}

impl AudioOutput {
    pub fn new() -> Self {
        AudioOutput {
            audio_stream: None,
            first_try: true,
        }
    }

    pub fn get_handle(&mut self) -> Option<&OutputStreamHandle> {
        if self.audio_stream.is_none() && self.first_try {
            self.first_try = false;
            if let Ok((stream, handle)) = OutputStream::try_default() {
                self.audio_stream = Some((stream, handle));
            } else {
                log::error!("Failed to open audio stream");
                return None;
            }
        }
        self.audio_stream.as_ref().map(|(_, h)| h)
    }
}
