#[cfg(not(feature = "wayvr"))]
compile_error!("WayVR feature is not enabled");

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::{load_known_yaml, ConfigType};

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRAppEntry {
    pub name: String,
    pub target_display: String,
    pub exec: String,
    pub args: Option<String>,
    pub env: Option<Vec<String>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRDisplay {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
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

#[derive(Deserialize, Serialize)]
pub struct WayVRConfig {
    pub version: u32,
    pub catalogs: HashMap<String, WayVRCatalog>,
    pub displays: HashMap<String, WayVRDisplay>,
}

impl WayVRConfig {
    pub fn get_catalog(&self, name: &str) -> Option<&WayVRCatalog> {
        self.catalogs.get(name)
    }

    pub fn get_display(&self, name: &str) -> Option<&WayVRDisplay> {
        self.displays.get(name)
    }
}

pub fn load_wayvr() -> WayVRConfig {
    let config = load_known_yaml::<WayVRConfig>(ConfigType::WayVR);
    if config.version != 1 {
        panic!("WayVR config version {} is not supported", config.version);
    }
    config
}
