#[cfg(not(feature = "wayvr"))]
compile_error!("WayVR feature is not enabled");

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use wgui::gfx::WGfx;
use wlx_common::{common::LeftRight, config::GeneralConfig, config_io, windowing::Positioning};

use crate::{
    backend::{
        task::TaskContainer,
        wayvr::{self, WvrServerState},
    },
    config::load_config_with_conf_d,
    graphics::WGfxExtras,
    ipc::{event_queue::SyncEventQueue, signal::WayVRSignal},
};

// Flat version of RelativeTo
#[derive(Clone, Deserialize, Serialize)]
pub enum AttachTo {
    None,
    HandLeft,
    HandRight,
    Head,
    Stage,
}

impl AttachTo {
    // TODO: adjustable lerp factor
    pub const fn get_positioning(&self) -> Positioning {
        match self {
            Self::None => Positioning::Floating,
            Self::HandLeft => Positioning::FollowHand {
                hand: LeftRight::Left,
                lerp: 1.0,
                align_to_hmd: false,
            },
            Self::HandRight => Positioning::FollowHand {
                hand: LeftRight::Right,
                lerp: 1.0,
                align_to_hmd: false,
            },
            Self::Stage => Positioning::Static,
            Self::Head => Positioning::FollowHead { lerp: 1.0 },
        }
    }

    pub const fn from_packet(input: &wayvr_ipc::packet_client::AttachTo) -> Self {
        match input {
            wayvr_ipc::packet_client::AttachTo::None => Self::None,
            wayvr_ipc::packet_client::AttachTo::HandLeft => Self::HandLeft,
            wayvr_ipc::packet_client::AttachTo::HandRight => Self::HandRight,
            wayvr_ipc::packet_client::AttachTo::Head => Self::Head,
            wayvr_ipc::packet_client::AttachTo::Stage => Self::Stage,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Rotation {
    pub axis: [f32; 3],
    pub angle: f32,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRAppEntry {
    pub name: String,
    pub target_display: String,
    pub exec: String,
    pub args: Option<String>,
    pub env: Option<Vec<String>>,
    pub shown_at_start: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRDisplay {
    pub width: u16,
    pub height: u16,
    pub scale: Option<f32>,
    pub rotation: Option<Rotation>,
    pub pos: Option<[f32; 3]>,
    pub attach_to: Option<AttachTo>,
    pub primary: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRCatalog {
    pub apps: Vec<WayVRAppEntry>,
}

impl WayVRCatalog {
    pub fn get_app(&self, name: &str) -> Option<&WayVRAppEntry> {
        self.apps.iter().find(|&app| app.name.as_str() == name)
    }
}

const fn def_false() -> bool {
    false
}

const fn def_true() -> bool {
    true
}

const fn def_autohide_delay() -> u32 {
    750
}

const fn def_keyboard_repeat_delay() -> u32 {
    200
}

const fn def_keyboard_repeat_rate() -> u32 {
    50
}

fn def_blit_method() -> String {
    String::from("dmabuf")
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRDashboard {
    pub exec: String,
    pub working_dir: Option<String>,
    pub args: Option<String>,
    pub env: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
pub struct WayVRConfig {
    #[serde(default = "Default::default")]
    pub catalogs: HashMap<String, WayVRCatalog>,

    #[serde(default = "Default::default")]
    pub displays: BTreeMap<String, WayVRDisplay>, // sorted alphabetically

    #[serde(default = "Default::default")]
    pub dashboard: Option<WayVRDashboard>,

    #[serde(default = "def_true")]
    pub auto_hide: bool,

    #[serde(default = "def_autohide_delay")]
    pub auto_hide_delay: u32,

    #[serde(default = "def_keyboard_repeat_delay")]
    pub keyboard_repeat_delay: u32,

    #[serde(default = "def_keyboard_repeat_rate")]
    pub keyboard_repeat_rate: u32,

    #[serde(default = "def_blit_method")]
    pub blit_method: String,
}

impl WayVRConfig {
    pub fn get_catalog(&self, name: &str) -> Option<&WayVRCatalog> {
        self.catalogs.get(name)
    }

    pub fn get_display(&self, name: &str) -> Option<&WayVRDisplay> {
        self.displays.get(name)
    }

    pub fn get_default_display(&self) -> Option<(String, &WayVRDisplay)> {
        for (disp_name, disp) in &self.displays {
            if disp.primary.unwrap_or(false) {
                return Some((disp_name.clone(), disp));
            }
        }
        None
    }

    pub fn get_wayvr_config(
        config_general: &GeneralConfig,
        config_wayvr: &Self,
    ) -> anyhow::Result<wayvr::Config> {
        Ok(wayvr::Config {
            click_freeze_time_ms: config_general.click_freeze_time_ms,
            keyboard_repeat_delay_ms: config_wayvr.keyboard_repeat_delay,
            keyboard_repeat_rate: config_wayvr.keyboard_repeat_rate,
            blit_method: wayvr::BlitMethod::from_string(&config_wayvr.blit_method)
                .context("unknown blit method")?,
            auto_hide_delay: if config_wayvr.auto_hide {
                Some(config_wayvr.auto_hide_delay)
            } else {
                None
            },
        })
    }

    pub fn post_load(
        &self,
        gfx: Arc<WGfx>,
        gfx_extras: &WGfxExtras,
        config: &GeneralConfig,
        _tasks: &mut TaskContainer,
        signals: SyncEventQueue<WayVRSignal>,
    ) -> anyhow::Result<WvrServerState> {
        let primary_count = self
            .displays
            .iter()
            .filter(|d| d.1.primary.unwrap_or(false))
            .count();

        if primary_count > 1 {
            anyhow::bail!("Number of primary displays is more than 1")
        }

        for (_catalog_name, catalog) in &self.catalogs {
            for app in &catalog.apps {
                if let Some(b) = app.shown_at_start
                    && b
                {
                    //CLEANUP: is this needed?
                }
            }
        }

        WvrServerState::new(
            gfx,
            gfx_extras,
            Self::get_wayvr_config(config, self)?,
            signals,
        )
    }
}

fn get_default_dashboard_exec() -> (
    String,         /* exec path */
    Option<String>, /* working directory */
) {
    if let Ok(appdir) = std::env::var("APPDIR") {
        // Running in AppImage
        let embedded_path = format!("{appdir}/usr/bin/wayvr-dashboard");
        if executable_exists_in_path(&embedded_path) {
            log::info!("Using WayVR Dashboard from AppDir: {embedded_path}");
            return (embedded_path, Some(format!("{appdir}/usr")));
        }
    }
    (String::from("wayvr-dashboard"), None)
}

pub fn executable_exists_in_path(command: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false; // very unlikely to happen
    };
    for dir in path.split(':') {
        let exec_path = std::path::PathBuf::from(dir).join(command);
        if exec_path.exists() && exec_path.is_file() {
            return true; // executable found
        }
    }
    false
}

pub fn load_wayvr() -> WayVRConfig {
    let config_root_path = config_io::ConfigRoot::WayVR.ensure_dir();
    log::info!("WayVR Config root path: {}", config_root_path.display());
    log::info!(
        "WayVR conf.d path: {}",
        config_io::ConfigRoot::WayVR.get_conf_d_path().display()
    );

    let mut conf =
        load_config_with_conf_d::<WayVRConfig>("wayvr.yaml", config_io::ConfigRoot::WayVR);

    if conf.dashboard.is_none() {
        let (exec, working_dir) = get_default_dashboard_exec();

        conf.dashboard = Some(WayVRDashboard {
            args: None,
            env: None,
            exec,
            working_dir,
        });
    }

    conf
}
