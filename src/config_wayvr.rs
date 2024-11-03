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
    overlays::wayvr::{WayVRAction, WayVRState},
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
    pub shown_at_start: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WayVRDisplay {
    pub width: u32,
    pub height: u32,
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

#[derive(Deserialize, Serialize)]
pub struct WayVRConfig {
    pub version: u32,
    pub run_compositor_at_start: bool,
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

    pub fn get_default_display(&self) -> Option<(String, &WayVRDisplay)> {
        for (disp_name, disp) in &self.displays {
            if disp.primary.unwrap_or(false) {
                return Some((disp_name.clone(), disp));
            }
        }
        None
    }

    pub fn get_wayvr_config(config: &crate::config::GeneralConfig) -> wayvr::Config {
        wayvr::Config {
            click_freeze_time_ms: config.click_freeze_time_ms,
            keyboard_repeat_delay_ms: 200,
            keyboard_repeat_rate: 50,
        }
    }

    pub fn post_load(
        &self,
        config: &crate::config::GeneralConfig,
        tasks: &mut TaskContainer,
    ) -> anyhow::Result<Option<Rc<RefCell<WayVRState>>>> {
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
            Ok(Some(Rc::new(RefCell::new(WayVRState::new(
                Self::get_wayvr_config(config),
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
