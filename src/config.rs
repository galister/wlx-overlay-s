use std::path::PathBuf;
use std::sync::Arc;

use crate::config_io;
use crate::config_io::get_conf_d_path;
use crate::config_io::CONFIG_ROOT_PATH;
use crate::gui::modular::ModularUiConfig;
use crate::overlays::toast::DisplayMethod;
use crate::overlays::toast::ToastTopic;
use crate::state::LeftRight;
use anyhow::bail;
use config::Config;
use config::File;
use glam::vec3a;
use glam::Affine3A;
use glam::Quat;
use glam::Vec3A;
use idmap::IdMap;
use log::error;
use serde::Deserialize;
use serde::Serialize;

pub type AStrMap<V> = Vec<(Arc<str>, V)>;

pub trait AStrMapExt<V> {
    fn arc_set(&mut self, key: Arc<str>, value: V) -> bool;
    fn arc_get(&self, key: &str) -> Option<&V>;
    fn arc_rm(&mut self, key: &str) -> Option<V>;
}

impl<V> AStrMapExt<V> for AStrMap<V> {
    fn arc_set(&mut self, key: Arc<str>, value: V) -> bool {
        let index = self.iter().position(|(k, _)| k.as_ref().eq(key.as_ref()));
        index.map(|i| self.remove(i).1);
        self.push((key, value));
        true
    }

    fn arc_get(&self, key: &str) -> Option<&V> {
        self.iter()
            .find_map(|(k, v)| if k.as_ref().eq(key) { Some(v) } else { None })
    }

    fn arc_rm(&mut self, key: &str) -> Option<V> {
        let index = self.iter().position(|(k, _)| k.as_ref().eq(key));
        index.map(|i| self.remove(i).1)
    }
}

pub type AStrSet = Vec<Arc<str>>;

pub trait AStrSetExt {
    fn arc_set(&mut self, value: Arc<str>) -> bool;
    fn arc_get(&self, value: &str) -> bool;
    fn arc_rm(&mut self, value: &str) -> bool;
}

impl AStrSetExt for AStrSet {
    fn arc_set(&mut self, value: Arc<str>) -> bool {
        if self.iter().any(|v| v.as_ref().eq(value.as_ref())) {
            return false;
        }
        self.push(value);
        true
    }

    fn arc_get(&self, value: &str) -> bool {
        self.iter().any(|v| v.as_ref().eq(value))
    }

    fn arc_rm(&mut self, value: &str) -> bool {
        let index = self.iter().position(|v| v.as_ref().eq(value));
        index
            .map(|i| {
                self.remove(i);
                true
            })
            .unwrap_or(false)
    }
}

pub type PwTokenMap = AStrMap<String>;

pub fn def_watch_pos() -> Vec3A {
    vec3a(-0.03, -0.01, 0.125)
}

pub fn def_watch_rot() -> Quat {
    Quat::from_xyzw(-0.7071066, 0.0007963618, 0.7071066, 0.0)
}

pub fn def_left() -> LeftRight {
    LeftRight::Left
}

pub fn def_pw_tokens() -> PwTokenMap {
    AStrMap::new()
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

pub fn def_half() -> f32 {
    0.5
}

pub fn def_point7() -> f32 {
    0.7
}

pub fn def_point3() -> f32 {
    0.3
}

fn def_osc_port() -> u16 {
    9000
}

fn def_screens() -> AStrSet {
    AStrSet::new()
}

fn def_curve_values() -> AStrMap<f32> {
    AStrMap::new()
}

fn def_transforms() -> AStrMap<Affine3A> {
    AStrMap::new()
}

fn def_auto() -> Arc<str> {
    "auto".into()
}

fn def_empty() -> Arc<str> {
    "".into()
}

fn def_toast_topics() -> IdMap<ToastTopic, DisplayMethod> {
    IdMap::new()
}

fn def_font() -> Arc<str> {
    "LiberationSans:style=Bold".into()
}

fn def_max_height() -> u16 {
    1440
}

#[derive(Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "def_watch_pos")]
    pub watch_pos: Vec3A,

    #[serde(default = "def_watch_rot")]
    pub watch_rot: Quat,

    #[serde(default = "def_left")]
    pub watch_hand: LeftRight,

    #[serde(default = "def_click_freeze_time_ms")]
    pub click_freeze_time_ms: u32,

    #[serde(default = "def_true")]
    pub notifications_enabled: bool,

    #[serde(default = "def_true")]
    pub notifications_sound_enabled: bool,

    #[serde(default = "def_toast_topics")]
    pub notification_topics: IdMap<ToastTopic, DisplayMethod>,

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

    #[serde(default = "def_osc_port")]
    pub osc_out_port: u16,

    #[serde(default = "def_false")]
    pub upright_screen_fix: bool,

    #[serde(default = "def_false")]
    pub double_cursor_fix: bool,

    #[serde(default = "def_screens")]
    pub show_screens: AStrSet,

    #[serde(default = "def_curve_values")]
    pub curve_values: AStrMap<f32>,

    #[serde(default = "def_transforms")]
    pub transform_values: AStrMap<Affine3A>,

    #[serde(default = "def_auto")]
    pub capture_method: Arc<str>,

    #[serde(default = "def_point7")]
    pub xr_grab_sensitivity: f32,

    #[serde(default = "def_point7")]
    pub xr_click_sensitivity: f32,

    #[serde(default = "def_point7")]
    pub xr_alt_click_sensitivity: f32,

    #[serde(default = "def_half")]
    pub xr_grab_sensitivity_release: f32,

    #[serde(default = "def_half")]
    pub xr_click_sensitivity_release: f32,

    #[serde(default = "def_half")]
    pub xr_alt_click_sensitivity_release: f32,

    #[serde(default = "def_true")]
    pub allow_sliding: bool,

    #[serde(default = "def_true")]
    pub realign_on_showhide: bool,

    #[serde(default = "def_false")]
    pub focus_follows_mouse_mode: bool,

    #[serde(default = "def_font")]
    pub primary_font: Arc<str>,

    #[serde(default = "def_one")]
    pub space_drag_multiplier: f32,

    #[serde(default = "def_empty")]
    pub skybox_texture: Arc<str>,

    #[serde(default = "def_true")]
    pub use_skybox: bool,

    #[serde(default = "def_max_height")]
    pub screen_max_height: u16,

    #[serde(default = "def_false")]
    pub screen_render_down: bool,

    #[serde(default = "def_point3")]
    pub pointer_lerp_factor: f32,

    #[serde(default = "def_false")]
    pub space_rotate_unlocked: bool,
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
        GeneralConfig::sanitize_range("keyboard_scale", self.keyboard_scale, 0.05, 5.0);
        GeneralConfig::sanitize_range("desktop_view_scale", self.desktop_view_scale, 0.05, 5.0);
    }
}

const FALLBACKS: [&str; 4] = [
    include_str!("res/keyboard.yaml"),
    include_str!("res/watch.yaml"),
    include_str!("res/settings.yaml"),
    include_str!("res/anchor.yaml"),
];

const FILES: [&str; 4] = [
    "keyboard.yaml",
    "watch.yaml",
    "settings.yaml",
    "anchor.yaml",
];

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum ConfigType {
    Keyboard,
    Watch,
    Settings,
    Anchor,
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
                error!("Failed to parse {}, falling back to defaults.", file_name);
                error!("{}", e);
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
    let mut settings_builder = Config::builder();

    // Add files from conf.d directory
    let path_conf_d = get_conf_d_path();

    for mut base_conf in [CONFIG_ROOT_PATH.clone(), path_conf_d.clone()] {
        base_conf.push("config.yaml");
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
                    error!("Failed to read conf.d directory: {}", e);
                    None
                }
            })
            .collect();
        // Sort paths alphabetically
        paths.sort_by_key(|dir| dir.path());
        for path in paths {
            log::info!("Loading config file: {}", path.path().to_string_lossy());
            settings_builder = settings_builder.add_source(File::from(path.path()));
        }
    }

    match settings_builder.build() {
        Ok(settings) => match settings.try_deserialize::<GeneralConfig>() {
            Ok(config) => config,
            Err(e) => {
                panic!("Failed to deserialize settings: {}", e);
            }
        },
        Err(e) => {
            panic!("Failed to build settings: {}", e);
        }
    }
}

// Config that is saved from the settings panel

#[derive(Serialize)]
pub struct AutoSettings {
    pub watch_pos: Vec3A,
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
    let mut path = config_io::get_conf_d_path();
    path.push("zz-saved-config.json5");
    path
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
    pub show_screens: AStrSet,
    pub curve_values: AStrMap<f32>,
    pub transform_values: AStrMap<Affine3A>,
}

fn get_state_path() -> PathBuf {
    let mut path = config_io::get_conf_d_path();
    path.push("zz-saved-state.json5");
    path
}

pub fn save_layout(config: &GeneralConfig) -> anyhow::Result<()> {
    let conf = AutoState {
        show_screens: config.show_screens.clone(),
        curve_values: config.curve_values.clone(),
        transform_values: config.transform_values.clone(),
    };

    let json = serde_json::to_string_pretty(&conf).unwrap(); // want panic
    std::fs::write(get_state_path(), json)?;

    Ok(())
}
