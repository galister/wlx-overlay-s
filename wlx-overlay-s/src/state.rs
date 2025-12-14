use glam::Affine3A;
use idmap::IdMap;
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;
use wgui::{
    font_config::WguiFontConfig, gfx::WGfx, globals::WguiGlobals, parser::parse_color_hex,
    renderer_vk::context::SharedContext as WSharedContext,
};
use wlx_common::{
    config::GeneralConfig,
    overlays::{ToastDisplayMethod, ToastTopic},
};

#[cfg(feature = "wayvr")]
use {
    crate::config_wayvr::{self, WayVRConfig},
    crate::overlays::wayvr::WayVRData,
    std::{cell::RefCell, rc::Rc},
};

#[cfg(feature = "osc")]
use crate::subsystem::osc::OscSender;

use crate::{
    backend::{input::InputState, task::TaskContainer},
    config::load_general_config,
    config_io::{self, get_config_file_path},
    graphics::WGfxExtras,
    gui,
    subsystem::{audio::AudioOutput, input::HidWrapper},
};

pub struct AppState {
    pub session: AppSession,
    pub tasks: TaskContainer,

    pub gfx: Arc<WGfx>,
    pub gfx_extras: WGfxExtras,
    pub hid_provider: HidWrapper,
    pub audio_provider: AudioOutput,

    pub wgui_shared: WSharedContext,

    pub input_state: InputState,
    pub screens: SmallVec<[ScreenMeta; 8]>,
    pub anchor: Affine3A,
    pub anchor_grabbed: bool,
    pub toast_sound: &'static [u8],

    pub wgui_globals: WguiGlobals,

    #[cfg(feature = "osc")]
    pub osc_sender: Option<OscSender>,

    #[cfg(feature = "wayvr")]
    pub wayvr: Option<Rc<RefCell<WayVRData>>>, // Dynamically created if requested
}

#[allow(unused_mut)]
impl AppState {
    pub fn from_graphics(gfx: Arc<WGfx>, gfx_extras: WGfxExtras) -> anyhow::Result<Self> {
        // insert shared resources
        let mut tasks = TaskContainer::new();

        let session = AppSession::load();

        #[cfg(feature = "wayvr")]
        let wayvr = session
            .wayvr_config
            .post_load(&session.config, &mut tasks)?;

        let mut hid_provider = HidWrapper::new();

        #[cfg(feature = "wayvr")]
        if let Some(wayvr) = wayvr.as_ref() {
            hid_provider.set_wayvr(wayvr.clone());
        }

        #[cfg(feature = "osc")]
        let osc_sender = crate::subsystem::osc::OscSender::new(session.config.osc_out_port).ok();

        let toast_sound_wav = Self::try_load_bytes(
            &session.config.notification_sound,
            include_bytes!("res/557297.wav"),
        );

        let wgui_shared = WSharedContext::new(gfx.clone())?;
        let theme = session.config.theme_path.clone();

        let mut defaults = wgui::globals::Defaults::default();
        defaults.accent_color = session
            .config
            .color_accent
            .as_ref()
            .and_then(|c| parse_color_hex(&c))
            .unwrap_or(defaults.accent_color);

        defaults.danger_color = session
            .config
            .color_danger
            .as_ref()
            .and_then(|c| parse_color_hex(&c))
            .unwrap_or(defaults.danger_color);

        defaults.faded_color = session
            .config
            .color_faded
            .as_ref()
            .and_then(|c| parse_color_hex(&c))
            .unwrap_or(defaults.faded_color);

        Ok(Self {
            session,
            tasks,
            gfx,
            gfx_extras,
            hid_provider,
            audio_provider: AudioOutput::new(),
            wgui_shared,
            input_state: InputState::new(),
            screens: smallvec![],
            anchor: Affine3A::IDENTITY,
            anchor_grabbed: false,
            toast_sound: toast_sound_wav,
            wgui_globals: WguiGlobals::new(
                Box::new(gui::asset::GuiAsset {}),
                defaults,
                &WguiFontConfig::default(),
                get_config_file_path(&theme),
            )?,

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
                WayVRConfig::get_wayvr_config(&self.session.config, &self.session.wayvr_config)?,
            )?));
            self.hid_provider.set_wayvr(wayvr.clone());
            self.wayvr = Some(wayvr.clone());
            Ok(wayvr)
        }
    }

    pub fn try_load_bytes(path: &str, fallback_data: &'static [u8]) -> &'static [u8] {
        if path.is_empty() {
            return fallback_data;
        }

        let real_path = config_io::get_config_root().join(path);

        if std::fs::File::open(real_path.clone()).is_err() {
            log::warn!("Could not open file at: {path}");
            return fallback_data;
        }

        match std::fs::read(real_path) {
            // Box is used here to work around `f`'s limited lifetime
            Ok(f) => Box::leak(Box::new(f)).as_slice(),
            Err(e) => {
                log::warn!("Failed to read file at: {path}");
                log::warn!("{e:?}");
                fallback_data
            }
        }
    }
}

pub struct AppSession {
    pub config: GeneralConfig,

    #[cfg(feature = "wayvr")]
    pub wayvr_config: WayVRConfig,

    pub toast_topics: IdMap<ToastTopic, ToastDisplayMethod>,
}

impl AppSession {
    pub fn load() -> Self {
        let config_root_path = config_io::ConfigRoot::Generic.ensure_dir();
        log::info!("Config root path: {}", config_root_path.display());
        let config = load_general_config();

        let mut toast_topics = IdMap::new();
        toast_topics.insert(ToastTopic::System, ToastDisplayMethod::Center);
        toast_topics.insert(ToastTopic::DesktopNotification, ToastDisplayMethod::Center);
        toast_topics.insert(ToastTopic::XSNotification, ToastDisplayMethod::Center);

        config.notification_topics.iter().for_each(|(k, v)| {
            toast_topics.insert(*k, *v);
        });

        #[cfg(feature = "wayvr")]
        let wayvr_config = config_wayvr::load_wayvr();

        Self {
            config,
            #[cfg(feature = "wayvr")]
            wayvr_config,
            toast_topics,
        }
    }
}

pub struct ScreenMeta {
    pub name: Arc<str>,
    pub native_handle: u32,
}
