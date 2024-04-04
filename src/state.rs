use std::{io::Cursor, path::PathBuf, sync::Arc};

use anyhow::bail;
use glam::Vec3;
use idmap::IdMap;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Source};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use vulkano::format::Format;

use crate::{
    backend::{common::TaskContainer, input::InputState},
    config::GeneralConfig,
    config_io,
    graphics::WlxGraphics,
    gui::font::FontCache,
    hid::HidProvider,
    overlays::toast::{DisplayMethod, ToastTopic},
    shaders::{frag_color, frag_glyph, frag_screen, frag_sprite, frag_srgb, vert_common},
};

pub struct AppState {
    pub fc: FontCache,
    pub session: AppSession,
    pub tasks: TaskContainer,
    pub graphics: Arc<WlxGraphics>,
    pub format: vulkano::format::Format,
    pub input_state: InputState,
    pub hid_provider: Box<dyn HidProvider>,
    pub audio: AudioOutput,
    pub screens: SmallVec<[ScreenMeta; 8]>,
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

            let shader = frag_screen::load(graphics.device.clone())?;
            shaders.insert("frag_screen", shader);

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
            screens: smallvec![],
        })
    }
}

pub struct AppSession {
    pub config_root_path: PathBuf,
    pub config: GeneralConfig,

    pub toast_topics: IdMap<ToastTopic, DisplayMethod>,

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

        let mut toast_topics = IdMap::new();
        toast_topics.insert(ToastTopic::System, DisplayMethod::Center);
        toast_topics.insert(ToastTopic::DesktopNotification, DisplayMethod::Center);
        toast_topics.insert(ToastTopic::XSNotification, DisplayMethod::Center);

        config.notification_topics.iter().for_each(|(k, v)| {
            toast_topics.insert(*k, *v);
        });

        AppSession {
            config_root_path,
            config,
            toast_topics,
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

    fn get_handle(&mut self) -> Option<&OutputStreamHandle> {
        if self.audio_stream.is_none() && self.first_try {
            self.first_try = false;
            if let Ok((stream, handle)) = OutputStream::try_default() {
                self.audio_stream = Some((stream, handle));
            } else {
                log::error!("Failed to open audio stream. Audio will not work.");
                return None;
            }
        }
        self.audio_stream.as_ref().map(|(_, h)| h)
    }

    pub fn play(&mut self, wav_bytes: &'static [u8]) {
        let Some(handle) = self.get_handle() else {
            return;
        };
        let cursor = Cursor::new(wav_bytes);
        let source = match Decoder::new_wav(cursor) {
            Ok(source) => source,
            Err(e) => {
                log::error!("Failed to play sound: {:?}", e);
                return;
            }
        };
        let _ = handle.play_raw(source.convert_samples());
    }
}

pub struct ScreenMeta {
    pub name: Arc<str>,
    pub id: usize,
    pub native_handle: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[repr(u8)]
pub enum LeftRight {
    #[default]
    Left,
    Right,
}
