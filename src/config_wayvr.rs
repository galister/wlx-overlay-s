#[cfg(not(feature = "wayvr"))]
compile_error!("WayVR feature is not enabled");

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::{
    backend::overlay::RelativeTo,
    config::{load_known_yaml, ConfigType},
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
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRDisplay {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub rotation: Option<Rotation>,
    pub pos: Option<[f32; 3]>,
    pub attach_to: Option<AttachTo>,
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
    pub displays: BTreeMap<String, WayVRDisplay>, // sorted alphabetically
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
