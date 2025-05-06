#[cfg(not(feature = "wayvr"))]
compile_error!("WayVR feature is not enabled");

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    backend::{
        overlay::Positioning,
        task::{TaskContainer, TaskType},
        wayvr,
    },
    config::load_config_with_conf_d,
    config_io,
    gui::modular::button::WayVRAction,
    overlays::wayvr::WayVRData,
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
            Self::HandLeft => Positioning::FollowHand { hand: 0, lerp: 1.0 },
            Self::HandRight => Positioning::FollowHand { hand: 1, lerp: 1.0 },
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
    pub args: Option<String>,
    pub env: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
pub struct WayVRConfig {
    #[serde(default = "def_false")]
    pub run_compositor_at_start: bool,

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
        config_general: &crate::config::GeneralConfig,
        config_wayvr: &Self,
    ) -> anyhow::Result<wayvr::Config> {
        Ok(wayvr::Config {
            click_freeze_time_ms: config_general.click_freeze_time_ms,
            keyboard_repeat_delay_ms: config_wayvr.keyboard_repeat_delay,
            keyboard_repeat_rate: config_wayvr.keyboard_repeat_rate,
            blit_method: wayvr::BlitMethod::from_string(&config_wayvr.blit_method)
                .ok_or_else(|| anyhow::anyhow!("Unknown blit method"))?,
            auto_hide_delay: if config_wayvr.auto_hide {
                Some(config_wayvr.auto_hide_delay)
            } else {
                None
            },
        })
    }

    pub fn post_load(
        &self,
        config: &crate::config::GeneralConfig,
        tasks: &mut TaskContainer,
    ) -> anyhow::Result<Option<Rc<RefCell<WayVRData>>>> {
        let primary_count = self
            .displays
            .iter()
            .filter(|d| d.1.primary.unwrap_or(false))
            .count();

        if primary_count > 1 {
            anyhow::bail!("Number of primary displays is more than 1")
        } else if primary_count == 0 {
            log::warn!(
                "No primary display specified. External Wayland applications will not be attached."
            );
        }

        for (catalog_name, catalog) in &self.catalogs {
            for app in &catalog.apps {
                if let Some(b) = app.shown_at_start {
                    if b {
                        tasks.enqueue(TaskType::WayVR(WayVRAction::AppClick {
                            catalog_name: Arc::from(catalog_name.as_str()),
                            app_name: Arc::from(app.name.as_str()),
                        }));
                    }
                }
            }
        }

        if self.run_compositor_at_start {
            // Start Wayland server instantly
            Ok(Some(Rc::new(RefCell::new(WayVRData::new(
                Self::get_wayvr_config(config, self)?,
            )?))))
        } else {
            // Lazy-init WayVR later if the user requested
            Ok(None)
        }
    }
}

pub fn load_wayvr() -> WayVRConfig {
    let config_root_path = config_io::ConfigRoot::WayVR.ensure_dir();
    log::info!("WayVR Config root path: {}", config_root_path.display());
    log::info!(
        "WayVR conf.d path: {}",
        config_io::ConfigRoot::WayVR.get_conf_d_path().display()
    );

    load_config_with_conf_d::<WayVRConfig>("wayvr.yaml", config_io::ConfigRoot::WayVR)
}
