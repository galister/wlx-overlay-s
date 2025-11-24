use crate::config_io;
use config::{Config, File};
use glam::{Quat, Vec3};
use log::error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wlx_common::{
    common::LeftRight,
    config::{GeneralConfig, SerializedWindowSet},
};

const FALLBACKS: [&str; 2] = [
    include_str!("res/keyboard.yaml"),
    include_str!("res/wayvr.yaml"),
];

const FILES: [&str; 2] = ["keyboard.yaml", "wayvr.yaml"];

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum ConfigType {
    Keyboard,
    Watch,
    Settings,
    Anchor,
    #[allow(dead_code)]
    WayVR,
}

pub fn load_known_yaml<T>(config_type: ConfigType) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let fallback = FALLBACKS[config_type as usize];
    let file_name = FILES[config_type as usize];
    let maybe_override = config_io::load(file_name);

    for yaml in [maybe_override.as_deref(), Some(fallback)].iter().flatten() {
        match serde_yaml::from_str::<T>(yaml) {
            Ok(d) => return d,
            Err(e) => {
                error!("Failed to parse {file_name}, falling back to defaults.");
                error!("{e}");
            }
        }
    }
    // can only get here if internal fallback is broken
    panic!("No usable config found.");
}

pub fn load_config_with_conf_d<ConfigData>(
    root_config_filename: &str,
    ctype: config_io::ConfigRoot,
) -> ConfigData
where
    ConfigData: for<'de> Deserialize<'de>,
{
    let mut settings_builder = Config::builder();

    // Add files from conf.d directory
    let path_conf_d = ctype.get_conf_d_path();

    for mut base_conf in [config_io::get_config_root(), path_conf_d.clone()] {
        base_conf.push(root_config_filename);
        if base_conf.exists() {
            log::info!("Loading config file: {}", base_conf.to_string_lossy());
            settings_builder = settings_builder.add_source(File::from(base_conf));
        }
    }

    if let Ok(paths_unsorted) = std::fs::read_dir(path_conf_d) {
        let mut paths: Vec<_> = paths_unsorted
            .filter_map(|r| match r {
                Ok(entry) => Some(entry),
                Err(e) => {
                    error!("Failed to read conf.d directory: {e}");
                    None
                }
            })
            .collect();
        // Sort paths alphabetically
        paths.sort_by_key(std::fs::DirEntry::path);
        for path in paths {
            log::info!("Loading config file: {}", path.path().to_string_lossy());
            settings_builder = settings_builder.add_source(File::from(path.path()));
        }
    }

    match settings_builder.build() {
        Ok(settings) => match settings.try_deserialize::<ConfigData>() {
            Ok(config) => config,
            Err(e) => {
                panic!("Failed to deserialize settings: {e}");
            }
        },
        Err(e) => {
            panic!("Failed to build settings: {e}");
        }
    }
}

pub fn load_general_config() -> GeneralConfig {
    load_config_with_conf_d::<GeneralConfig>("config.yaml", config_io::ConfigRoot::Generic)
}

#[derive(Serialize)]
pub struct AutoSettings {
    pub watch_pos: Vec3,
    pub watch_rot: Quat,
    pub watch_hand: LeftRight,
    pub watch_view_angle_min: f32,
    pub watch_view_angle_max: f32,
    pub notifications_enabled: bool,
    pub notifications_sound_enabled: bool,
    pub realign_on_showhide: bool,
    pub allow_sliding: bool,
    pub space_drag_multiplier: f32,
}

fn get_settings_path() -> PathBuf {
    config_io::ConfigRoot::Generic
        .get_conf_d_path()
        .join("zz-saved-config.json5")
}

pub fn save_settings(config: &GeneralConfig) -> anyhow::Result<()> {
    let conf = AutoSettings {
        watch_pos: config.watch_pos,
        watch_rot: config.watch_rot,
        watch_hand: config.watch_hand,
        watch_view_angle_min: config.watch_view_angle_min,
        watch_view_angle_max: config.watch_view_angle_max,
        notifications_enabled: config.notifications_enabled,
        notifications_sound_enabled: config.notifications_sound_enabled,
        realign_on_showhide: config.realign_on_showhide,
        allow_sliding: config.allow_sliding,
        space_drag_multiplier: config.space_drag_multiplier,
    };

    let json = serde_json::to_string_pretty(&conf).unwrap(); // want panic
    std::fs::write(get_settings_path(), json)?;

    Ok(())
}

// Config that is saved after manipulating overlays

#[derive(Serialize)]
pub struct AutoState {
    pub sets: Vec<SerializedWindowSet>,
    pub last_set: u32,
}

fn get_state_path() -> PathBuf {
    config_io::ConfigRoot::Generic
        .get_conf_d_path()
        .join("zz-saved-state.json5")
}

pub fn save_state(config: &GeneralConfig) -> anyhow::Result<()> {
    let conf = AutoState {
        sets: config.sets.clone(),
        last_set: config.last_set,
    };

    let json = serde_json::to_string_pretty(&conf).unwrap(); // want panic
    std::fs::write(get_state_path(), json)?;

    log::info!("State was saved successfully.");
    Ok(())
}
