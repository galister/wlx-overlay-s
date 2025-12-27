use glam::Affine3A;
use idmap::IdMap;
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;
use wgui::{
    drawing, font_config::WguiFontConfig, gfx::WGfx, globals::WguiGlobals, parser::parse_color_hex,
    renderer_vk::context::SharedContext as WSharedContext,
};
use wlx_common::{
    config::GeneralConfig,
    overlays::{ToastDisplayMethod, ToastTopic},
};

#[cfg(feature = "wayvr")]
use crate::config_wayvr::{self, WayVRConfig};

#[cfg(feature = "wayvr")]
use crate::backend::wayvr::WvrServerState;
#[cfg(feature = "osc")]
use crate::subsystem::osc::OscSender;

use crate::{
    backend::{XrBackend, input::InputState, task::TaskContainer},
    config::load_general_config,
    config_io::{self, get_config_file_path},
    graphics::WGfxExtras,
    gui,
    ipc::{event_queue::SyncEventQueue, ipc_server, signal::WayVRSignal},
    subsystem::{audio::AudioOutput, dbus::DbusConnector, input::HidWrapper},
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

    pub dbus: DbusConnector,

    pub xr_backend: XrBackend,

    pub ipc_server: ipc_server::WayVRServer,
    pub wayvr_signals: SyncEventQueue<WayVRSignal>,

    #[cfg(feature = "osc")]
    pub osc_sender: Option<OscSender>,

    #[cfg(feature = "wayvr")]
    pub wvr_server: Option<WvrServerState>,
}

#[allow(unused_mut)]
impl AppState {
    pub fn from_graphics(
        gfx: Arc<WGfx>,
        gfx_extras: WGfxExtras,
        xr_backend: XrBackend,
    ) -> anyhow::Result<Self> {
        // insert shared resources
        let mut tasks = TaskContainer::new();

        let session = AppSession::load();
        let wayvr_signals = SyncEventQueue::new();

        #[cfg(feature = "wayvr")]
        let wayvr_server = session
            .wayvr_config
            .post_load(
                gfx.clone(),
                &gfx_extras,
                &session.config,
                &mut tasks,
                wayvr_signals.clone(),
            )
            .inspect_err(|e| log::error!("Could not initialize wayland server: {e:?}"))
            .ok();

        let mut hid_provider = HidWrapper::new();

        #[cfg(feature = "osc")]
        let osc_sender = crate::subsystem::osc::OscSender::new(session.config.osc_out_port).ok();

        let toast_sound_wav = Self::try_load_bytes(
            &session.config.notification_sound,
            include_bytes!("res/557297.wav"),
        );

        let wgui_shared = WSharedContext::new(gfx.clone())?;
        let theme = session.config.theme_path.clone();

        let mut defaults = wgui::globals::Defaults::default();

        fn apply_color(default: &mut drawing::Color, value: &Option<String>) {
            if let Some(parsed) = value.as_ref().and_then(|c| parse_color_hex(c)) {
                *default = parsed;
            }
        }

        apply_color(&mut defaults.text_color, &session.config.color_text);
        apply_color(&mut defaults.accent_color, &session.config.color_accent);
        apply_color(&mut defaults.danger_color, &session.config.color_danger);
        apply_color(&mut defaults.faded_color, &session.config.color_faded);
        apply_color(&mut defaults.bg_color, &session.config.color_background);

        defaults.animation_mult = 1. / session.config.animation_speed;
        defaults.rounding_mult = session.config.round_multiplier;

        let dbus = DbusConnector::default();

        let ipc_server = ipc_server::WayVRServer::new()?;

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
            dbus,
            xr_backend,
            ipc_server,
            wayvr_signals,

            #[cfg(feature = "osc")]
            osc_sender,

            #[cfg(feature = "wayvr")]
            wvr_server: wayvr_server,
        })
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
    pub wayvr_config: WayVRConfig, // TODO: rename to "wayland_server_config"

    pub toast_topics: IdMap<ToastTopic, ToastDisplayMethod>,
}

impl AppSession {
    pub fn load() -> Self {
        let config_root_path = config_io::ConfigRoot::Generic.ensure_dir();
        log::info!("Config root path: {}", config_root_path.display());
        let config = load_general_config();

        let mut toast_topics = IdMap::new();
        toast_topics.insert(ToastTopic::System, ToastDisplayMethod::Center);
        toast_topics.insert(ToastTopic::Error, ToastDisplayMethod::Center);
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
