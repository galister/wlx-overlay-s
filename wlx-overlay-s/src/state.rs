use glam::Affine3A;
use idmap::IdMap;
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;
use wgui::{
    drawing, font_config::WguiFontConfig, gfx::WGfx, globals::WguiGlobals, parser::parse_color_hex,
    renderer_vk::context::SharedContext as WSharedContext,
};
use wlx_common::{
    audio,
    config::GeneralConfig,
    config_io::{self, get_config_file_path},
    desktop_finder::DesktopFinder,
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
    graphics::WGfxExtras,
    gui,
    ipc::{event_queue::SyncEventQueue, ipc_server, signal::WayVRSignal},
    subsystem::{dbus::DbusConnector, input::HidWrapper},
};

pub struct AppState {
    pub session: AppSession,
    pub tasks: TaskContainer,

    pub gfx: Arc<WGfx>,
    pub gfx_extras: WGfxExtras,
    pub hid_provider: HidWrapper,

    pub audio_system: audio::AudioSystem,
    pub audio_sample_player: audio::SamplePlayer,

    pub wgui_shared: WSharedContext,

    pub input_state: InputState,
    pub screens: SmallVec<[ScreenMeta; 8]>,
    pub anchor: Affine3A,
    pub anchor_grabbed: bool,

    pub wgui_globals: WguiGlobals,

    pub dbus: DbusConnector,

    pub xr_backend: XrBackend,

    pub ipc_server: ipc_server::WayVRServer,
    pub wayvr_signals: SyncEventQueue<WayVRSignal>,

    pub desktop_finder: DesktopFinder,

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

        let wgui_shared = WSharedContext::new(gfx.clone())?;
        let theme = session.config.theme_path.clone();

        let mut audio_sample_player = audio::SamplePlayer::new();
        audio_sample_player.register_sample(
            "key_click",
            audio::AudioSample::from_mp3(include_bytes!("res/key_click.mp3"))?,
        );

        audio_sample_player.register_sample(
            "toast",
            audio::AudioSample::from_mp3(include_bytes!("res/toast.mp3"))?,
        );

        let mut assets = Box::new(gui::asset::GuiAsset {});
        audio_sample_player.register_wgui_samples(assets.as_mut())?;

        let mut defaults = wgui::globals::Defaults::default();

        {
            #[allow(clippy::ref_option)]
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
        }

        defaults.animation_mult = 1. / session.config.animation_speed;
        defaults.rounding_mult = session.config.round_multiplier;

        let dbus = DbusConnector::default();

        let ipc_server = ipc_server::WayVRServer::new()?;

        let mut desktop_finder = DesktopFinder::new();
        desktop_finder.refresh();

        Ok(Self {
            session,
            tasks,
            gfx,
            gfx_extras,
            hid_provider,
            audio_system: audio::AudioSystem::new(),
            audio_sample_player,
            wgui_shared,
            input_state: InputState::new(),
            screens: smallvec![],
            anchor: Affine3A::IDENTITY,
            anchor_grabbed: false,
            wgui_globals: WguiGlobals::new(
                assets,
                defaults,
                &WguiFontConfig::default(),
                get_config_file_path(&theme),
            )?,
            dbus,
            xr_backend,
            ipc_server,
            wayvr_signals,
            desktop_finder,

            #[cfg(feature = "osc")]
            osc_sender,

            #[cfg(feature = "wayvr")]
            wvr_server: wayvr_server,
        })
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
