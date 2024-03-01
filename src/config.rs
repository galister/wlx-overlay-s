use std::sync::Arc;

use crate::config_io;
use crate::config_io::get_conf_d_path;
use crate::gui::modular::ModularUiConfig;
use crate::load_with_fallback;
use crate::state::LeftRight;
use anyhow::bail;
use log::error;
use serde::Deserialize;
use serde::Serialize;

pub fn def_watch_pos() -> [f32; 3] {
    [-0.03, -0.01, 0.125]
}

pub fn def_watch_rot() -> [f32; 4] {
    [-0.7071066, 0.0007963618, 0.7071066, 0.0]
}

pub fn def_left() -> LeftRight {
    LeftRight::Left
}

pub fn def_pw_tokens() -> Vec<(String, String)> {
    Vec::new()
}

fn def_click_freeze_time_ms() -> u32 {
    300
}

pub fn def_true() -> bool {
    true
}

fn def_false() -> bool {
    false
}

fn def_one() -> f32 {
    1.0
}

fn def_half() -> f32 {
    0.5
}

fn def_point7() -> f32 {
    0.7
}

fn def_osc_port() -> u16 {
    9000
}

fn def_screens() -> Vec<Arc<str>> {
    vec![]
}

fn def_auto() -> Arc<str> {
    "auto".into()
}

#[derive(Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "def_watch_pos")]
    pub watch_pos: [f32; 3],

    #[serde(default = "def_watch_rot")]
    pub watch_rot: [f32; 4],

    #[serde(default = "def_left")]
    pub watch_hand: LeftRight,

    #[serde(default = "def_click_freeze_time_ms")]
    pub click_freeze_time_ms: u32,

    #[serde(default = "def_true")]
    pub notifications_enabled: bool,

    #[serde(default = "def_true")]
    pub notifications_sound_enabled: bool,

    #[serde(default = "def_true")]
    pub keyboard_sound_enabled: bool,

    #[serde(default = "def_one")]
    pub keyboard_scale: f32,

    #[serde(default = "def_one")]
    pub desktop_view_scale: f32,

    #[serde(default = "def_half")]
    pub watch_view_angle_min: f32,

    #[serde(default = "def_point7")]
    pub watch_view_angle_max: f32,

    #[serde(default = "def_one")]
    pub long_press_duration: f32,

    #[serde(default = "def_pw_tokens")]
    pub pw_tokens: Vec<(String, String)>,

    #[serde(default = "def_osc_port")]
    pub osc_out_port: u16,

    #[serde(default = "def_false")]
    pub upright_screen_fix: bool,

    #[serde(default = "def_false")]
    pub double_cursor_fix: bool,

    #[serde(default = "def_screens")]
    pub show_screens: Vec<Arc<str>>,

    #[serde(default = "def_auto")]
    pub capture_method: Arc<str>,
}

impl GeneralConfig {
    fn sanitize_range(name: &str, val: f32, from: f32, to: f32) {
        if !val.is_normal() || val < from || val > to {
            panic!(
                "GeneralConfig: {} needs to be between {} and {}",
                name, from, to
            );
        }
    }

    pub fn load_from_disk() -> GeneralConfig {
        let config = load_general();
        config.post_load();
        config
    }

    fn post_load(&self) {
        GeneralConfig::sanitize_range("keyboard_scale", self.keyboard_scale, 0.0, 5.0);
        GeneralConfig::sanitize_range("desktop_view_scale", self.desktop_view_scale, 0.0, 5.0);
    }
}

const FALLBACKS: [&str; 3] = [
    include_str!("res/keyboard.yaml"),
    include_str!("res/watch.yaml"),
    include_str!("res/settings.yaml"),
];

const FILES: [&str; 3] = ["keyboard.yaml", "watch.yaml", "settings.yaml"];

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum ConfigType {
    Keyboard,
    Watch,
    Settings,
}

pub fn load_known_yaml<T>(config_type: ConfigType) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let fallback = FALLBACKS[config_type as usize];
    let file_name = FILES[config_type as usize];
    let maybe_override = config_io::load(file_name);

    for yaml in [maybe_override.as_deref(), Some(fallback)].iter() {
        if let Some(yaml_data) = yaml {
            match serde_yaml::from_str::<T>(yaml_data) {
                Ok(d) => return d,
                Err(e) => {
                    error!("Failed to parse {}, falling back to defaults.", file_name);
                    error!("{}", e);
                }
            }
        }
    }
    // can only get here if internal fallback is broken
    panic!("No usable config found.");
}

pub fn load_custom_ui(name: &str) -> anyhow::Result<ModularUiConfig> {
    let filename = format!("{}.yaml", name);
    let Some(yaml_data) = config_io::load(&filename) else {
        bail!("Could not read file at {}", &filename);
    };
    Ok(serde_yaml::from_str(&yaml_data)?)
}

pub fn load_general() -> GeneralConfig {
    let mut yaml_data = String::new();

    // Add files from conf.d directory
    let path_conf_d = get_conf_d_path();
    if let Ok(paths_unsorted) = std::fs::read_dir(path_conf_d) {
        // Sort paths alphabetically
        let mut paths: Vec<_> = paths_unsorted.map(|r| r.unwrap()).collect(); // TODO safe unwrap?
        paths.sort_by_key(|dir| dir.path());
        for path in paths {
            if !path.file_type().unwrap().is_file() {
                // TODO safe unwrap?
                continue;
            }

            log::info!("Loading config file {}", path.path().to_string_lossy());

            if let Ok(data) = std::fs::read_to_string(path.path()) {
                yaml_data.push('\n'); // Just in case, if end of the config file was not newline
                yaml_data.push_str(data.as_str());
            } else {
                // Shouldn't happen anyways
                error!("Failed to load {}", path.path().to_string_lossy());
            }
        }
    }

    if yaml_data.is_empty() {
        yaml_data.push_str(load_with_fallback!("config.yaml", "res/config.yaml").as_str());
    }

    serde_yaml::from_str(&yaml_data).expect("Failed to parse config.yaml")
}
