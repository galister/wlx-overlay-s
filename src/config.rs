use crate::config_io;
use crate::config_io::get_conf_d_path;
use crate::load_with_fallback;
use crate::overlays::keyboard;
use crate::overlays::watch::WatchConfig;
use log::error;
use serde::Deserialize;
use serde::Serialize;

pub fn def_pw_tokens() -> Vec<(String, String)> {
    Vec::new()
}

fn def_click_freeze_time_ms() -> u32 {
    300
}

fn def_true() -> bool {
    true
}

fn def_one() -> f32 {
    1.0
}

fn def_osc_port() -> u16 {
    9000
}

#[derive(Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "def_click_freeze_time_ms")]
    pub click_freeze_time_ms: u32,

    #[serde(default = "def_true")]
    pub keyboard_sound_enabled: bool,

    #[serde(default = "def_one")]
    pub keyboard_scale: f32,

    #[serde(default = "def_one")]
    pub desktop_view_scale: f32,

    #[serde(default = "def_one")]
    pub watch_scale: f32,

    #[serde(default = "def_pw_tokens")]
    pub pw_tokens: Vec<(String, String)>,

    #[serde(default = "def_osc_port")]
    pub osc_out_port: u16,
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
        GeneralConfig::sanitize_range("watch_scale", self.watch_scale, 0.0, 5.0);
    }
}

pub fn load_keyboard() -> keyboard::Layout {
    let yaml_data = load_with_fallback!("keyboard.yaml", "res/keyboard.yaml");
    serde_yaml::from_str(&yaml_data).expect("Failed to parse keyboard.yaml")
}

pub fn load_watch() -> WatchConfig {
    let yaml_data = load_with_fallback!("watch.yaml", "res/watch.yaml");
    serde_yaml::from_str(&yaml_data).expect("Failed to parse watch.yaml")
}

pub fn load_general() -> GeneralConfig {
    let mut yaml_data = String::new();

    // Add files from conf.d directory
    let path_conf_d = get_conf_d_path();
    if let Ok(paths_unsorted) = std::fs::read_dir(path_conf_d) {
        // Sort paths alphabetically
        let mut paths: Vec<_> = paths_unsorted.map(|r| r.unwrap()).collect();
        paths.sort_by_key(|dir| dir.path());
        for path in paths {
            if !path.file_type().unwrap().is_file() {
                continue;
            }

            println!("Loading config file {}", path.path().to_string_lossy());

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
