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
        overlay::RelativeTo,
        task::{TaskContainer, TaskType},
        wayvr,
    },
    config::{load_known_yaml, ConfigType},
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
    pub fn get_relative_to(&self) -> RelativeTo {
        match self {
            AttachTo::None => RelativeTo::None,
            AttachTo::HandLeft => RelativeTo::Hand(0),
            AttachTo::HandRight => RelativeTo::Hand(1),
            AttachTo::Stage => RelativeTo::Stage,
            AttachTo::Head => RelativeTo::Head,
        }
    }

    pub fn from_packet(input: &wayvr_ipc::packet_client::AttachTo) -> AttachTo {
        match input {
            wayvr_ipc::packet_client::AttachTo::None => AttachTo::None,
            wayvr_ipc::packet_client::AttachTo::HandLeft => AttachTo::HandLeft,
            wayvr_ipc::packet_client::AttachTo::HandRight => AttachTo::HandRight,
            wayvr_ipc::packet_client::AttachTo::Head => AttachTo::Head,
            wayvr_ipc::packet_client::AttachTo::Stage => AttachTo::Stage,
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

fn def_true() -> bool {
    true
}

fn def_autohide_delay() -> u32 {
    750
}

fn def_keyboard_repeat_delay() -> u32 {
    200
}

fn def_keyboard_repeat_rate() -> u32 {
    50
}

#[derive(Deserialize, Serialize)]
pub struct WayVRDashboard {
    pub exec: String,
    pub args: Option<String>,
    pub env: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
pub struct WayVRConfig {
    pub version: u32,
    pub run_compositor_at_start: bool,
    pub catalogs: HashMap<String, WayVRCatalog>,
    pub displays: BTreeMap<String, WayVRDisplay>, // sorted alphabetically
    pub dashboard: WayVRDashboard,

    #[serde(default = "def_true")]
    pub auto_hide: bool,

    #[serde(default = "def_autohide_delay")]
    pub auto_hide_delay: u32,

    #[serde(default = "def_keyboard_repeat_delay")]
    pub keyboard_repeat_delay: u32,

    #[serde(default = "def_keyboard_repeat_rate")]
    pub keyboard_repeat_rate: u32,
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
        config_wayvr: &crate::config_wayvr::WayVRConfig,
    ) -> wayvr::Config {
        wayvr::Config {
            click_freeze_time_ms: config_general.click_freeze_time_ms,
            keyboard_repeat_delay_ms: config_wayvr.keyboard_repeat_delay,
            keyboard_repeat_rate: config_wayvr.keyboard_repeat_rate,
            auto_hide_delay: if config_wayvr.auto_hide {
                Some(config_wayvr.auto_hide_delay)
            } else {
                None
            },
        }
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
            log::warn!("No primary display specified");
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
                Self::get_wayvr_config(config, self),
            )?))))
        } else {
            // Lazy-init WayVR later if the user requested
            Ok(None)
        }
    }
}

pub fn load_wayvr() -> WayVRConfig {
    let config = load_known_yaml::<WayVRConfig>(ConfigType::WayVR);
    if config.version != 1 {
        panic!("WayVR config version {} is not supported", config.version);
    }
    config
}
