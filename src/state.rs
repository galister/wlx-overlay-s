use anyhow::bail;
use glam::Affine3A;
use idmap::IdMap;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Source};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::{io::Cursor, sync::Arc};
use vulkano::image::view::ImageView;

#[cfg(feature = "wayvr")]
use {
    crate::config_wayvr::{self, WayVRConfig},
    crate::overlays::wayvr::WayVRData,
    std::{cell::RefCell, rc::Rc},
};

#[cfg(feature = "osc")]
use crate::backend::osc::OscSender;

use crate::{
    backend::{input::InputState, overlay::OverlayID, task::TaskContainer},
    config::{AStrMap, GeneralConfig},
    config_io,
    graphics::WlxGraphics,
    gui::font::FontCache,
    hid::HidProvider,
    overlays::toast::{DisplayMethod, ToastTopic},
    shaders::{
        frag_color, frag_glyph, frag_grid, frag_screen, frag_sprite, frag_sprite2, frag_sprite2_hl,
        frag_swapchain, vert_common,
    },
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyboardFocus {
    PhysicalScreen,

    #[allow(dead_code)] // Not available if "wayvr" feature is disabled
    WayVR, // (for now without wayland window id data, it's handled internally),
}

pub struct AppState {
    pub fc: FontCache,
    pub session: AppSession,
    pub tasks: TaskContainer,
    pub graphics: Arc<WlxGraphics>,
    pub input_state: InputState,
    pub hid_provider: Box<dyn HidProvider>,
    pub audio: AudioOutput,
    pub screens: SmallVec<[ScreenMeta; 8]>,
    pub anchor: Affine3A,
    pub sprites: AStrMap<Arc<ImageView>>,
    pub keyboard_focus: KeyboardFocus,
    pub notification_sound: Arc<[u8]>,

    #[cfg(feature = "osc")]
    pub osc_sender: Option<OscSender>,

    #[cfg(feature = "wayvr")]
    pub wayvr: Option<Rc<RefCell<WayVRData>>>, // Dynamically created if requested
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

            let shader = frag_grid::load(graphics.device.clone())?;
            shaders.insert("frag_grid", shader);

            let shader = frag_sprite::load(graphics.device.clone())?;
            shaders.insert("frag_sprite", shader);

            let shader = frag_sprite2::load(graphics.device.clone())?;
            shaders.insert("frag_sprite2", shader);

            let shader = frag_sprite2_hl::load(graphics.device.clone())?;
            shaders.insert("frag_sprite2_hl", shader);

            let shader = frag_screen::load(graphics.device.clone())?;
            shaders.insert("frag_screen", shader);

            let shader = frag_swapchain::load(graphics.device.clone())?;
            shaders.insert("frag_swapchain", shader);
        }

        #[cfg(feature = "wayvr")]
        let mut tasks = TaskContainer::new();

        #[cfg(not(feature = "wayvr"))]
        let tasks = TaskContainer::new();

        let session = AppSession::load();

        #[cfg(feature = "wayvr")]
        let wayvr = session
            .wayvr_config
            .post_load(&session.config, &mut tasks)?;

        #[cfg(feature = "osc")]
        let osc_sender = crate::backend::osc::OscSender::new(session.config.osc_out_port).ok();

        let real_path = config_io::get_config_root().join(&*session.config.notification_sound);
        let notification_sound_wav = std::fs::read(real_path)?;

        Ok(AppState {
            fc: FontCache::new(session.config.primary_font.clone())?,
            session,
            tasks,
            graphics,
            input_state: InputState::new(),
            hid_provider: crate::hid::initialize(),
            audio: AudioOutput::new(),
            screens: smallvec![],
            anchor: Affine3A::IDENTITY,
            sprites: AStrMap::new(),
            keyboard_focus: KeyboardFocus::PhysicalScreen,
            notification_sound: notification_sound_wav.into(),

            #[cfg(feature = "osc")]
            osc_sender,

            #[cfg(feature = "wayvr")]
            wayvr,
        })
    }

    #[cfg(feature = "wayvr")]
    #[allow(dead_code)]
    pub fn get_wayvr(&mut self) -> anyhow::Result<Rc<RefCell<WayVRData>>> {
        if let Some(wvr) = &self.wayvr {
            Ok(wvr.clone())
        } else {
            let wayvr = Rc::new(RefCell::new(WayVRData::new(
                WayVRConfig::get_wayvr_config(&self.session.config, &self.session.wayvr_config),
            )?));
            self.wayvr = Some(wayvr.clone());
            Ok(wayvr)
        }
    }
}

pub struct AppSession {
    pub config: GeneralConfig,

    #[cfg(feature = "wayvr")]
    pub wayvr_config: WayVRConfig,

    pub toast_topics: IdMap<ToastTopic, DisplayMethod>,
}

impl AppSession {
    pub fn load() -> Self {
        let config_root_path = config_io::ConfigRoot::Generic.ensure_dir();
        log::info!("Config root path: {:?}", config_root_path);
        let config = GeneralConfig::load_from_disk();

        let mut toast_topics = IdMap::new();
        toast_topics.insert(ToastTopic::System, DisplayMethod::Center);
        toast_topics.insert(ToastTopic::DesktopNotification, DisplayMethod::Center);
        toast_topics.insert(ToastTopic::XSNotification, DisplayMethod::Center);

        config.notification_topics.iter().for_each(|(k, v)| {
            toast_topics.insert(*k, *v);
        });

        #[cfg(feature = "wayvr")]
        let wayvr_config = config_wayvr::load_wayvr();

        AppSession {
            config,
            toast_topics,
            #[cfg(feature = "wayvr")]
            wayvr_config,
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
    pub id: OverlayID,
    pub native_handle: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[repr(u8)]
pub enum LeftRight {
    #[default]
    Left,
    Right,
}
