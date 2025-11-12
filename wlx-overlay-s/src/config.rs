use std::path::PathBuf;
use std::sync::Arc;

use crate::config_io;
use crate::overlays::toast::{DisplayMethod, ToastTopic};
use crate::state::LeftRight;
use crate::windowing::set::SerializedWindowSet;
use chrono::Offset;
use config::{Config, File};
use glam::{Affine3A, Quat, Vec3, vec3};
use idmap::IdMap;
use log::error;
use serde::{Deserialize, Serialize};

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
        index.is_some_and(|i| {
            self.remove(i);
            true
        })
    }
}

pub type PwTokenMap = AStrMap<String>;

pub const fn def_watch_pos() -> Vec3 {
    vec3(-0.03, -0.01, 0.125)
}

pub const fn def_watch_rot() -> Quat {
    Quat::from_xyzw(-0.707_106_6, 0.000_796_361_8, 0.707_106_6, 0.0)
}

pub const fn def_left() -> LeftRight {
    LeftRight::Left
}

pub const fn def_pw_tokens() -> PwTokenMap {
    AStrMap::new()
}

const fn def_mouse_move_interval_ms() -> u32 {
    10 // 100fps
}

const fn def_click_freeze_time_ms() -> u32 {
    300
}

pub const fn def_true() -> bool {
    true
}

const fn def_false() -> bool {
    false
}

const fn def_one() -> f32 {
    1.0
}

pub const fn def_half() -> f32 {
    0.5
}

pub const fn def_point7() -> f32 {
    0.7
}

pub const fn def_point3() -> f32 {
    0.3
}

const fn def_osc_port() -> u16 {
    9000
}

const fn def_empty_vec_string() -> Vec<String> {
    Vec::new()
}

const fn def_sets() -> Vec<SerializedWindowSet> {
    Vec::new()
}

const fn def_zero_u32() -> u32 {
    0
}

fn def_timezones() -> Vec<String> {
    const EMEA: i32 = -60 * 60; // UTC-1
    const APAC: i32 = 5 * 60 * 60; // UTC+5

    let offset = chrono::Local::now().offset().fix();
    match offset.local_minus_utc() {
        i32::MIN..EMEA => vec!["Europe/Paris".into(), "Asia/Tokyo".into()],
        EMEA..APAC => vec!["America/New_York".into(), "Asia/Tokyo".into()],
        APAC..=i32::MAX => vec!["Europe/Paris".into(), "America/New_York".into()],
    }
}

const fn def_screens() -> AStrSet {
    AStrSet::new()
}

const fn def_curve_values() -> AStrMap<f32> {
    AStrMap::new()
}

const fn def_transforms() -> AStrMap<Affine3A> {
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

const fn def_max_height() -> u16 {
    1440
}

#[derive(Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "def_watch_pos")]
    pub watch_pos: Vec3,

    #[serde(default = "def_watch_rot")]
    pub watch_rot: Quat,

    #[serde(default = "def_left")]
    pub watch_hand: LeftRight,

    #[serde(default = "def_click_freeze_time_ms")]
    pub click_freeze_time_ms: u32,

    #[serde(default = "def_false")]
    pub invert_scroll_direction_x: bool,

    #[serde(default = "def_false")]
    pub invert_scroll_direction_y: bool,

    #[serde(default = "def_one")]
    pub scroll_speed: f32,

    #[serde(default = "def_mouse_move_interval_ms")]
    pub mouse_move_interval_ms: u32,

    #[serde(default = "def_true")]
    pub notifications_enabled: bool,

    #[serde(default = "def_true")]
    pub notifications_sound_enabled: bool,

    #[serde(default = "def_toast_topics")]
    pub notification_topics: IdMap<ToastTopic, DisplayMethod>,

    #[serde(default = "def_empty")]
    pub notification_sound: Arc<str>,

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

    #[serde(default = "def_false")]
    pub block_game_input: bool,

    #[serde(default = "def_true")]
    pub block_game_input_ignore_watch: bool,

    #[serde(default = "def_font")]
    pub primary_font: Arc<str>,

    #[serde(default = "def_one")]
    pub space_drag_multiplier: f32,

    #[serde(default = "def_empty")]
    pub skybox_texture: Arc<str>,

    #[serde(default = "def_true")]
    pub use_skybox: bool,

    #[serde(default = "def_true")]
    pub use_passthrough: bool,

    #[serde(default = "def_max_height")]
    pub screen_max_height: u16,

    #[serde(default = "def_false")]
    pub screen_render_down: bool,

    #[serde(default = "def_point3")]
    pub pointer_lerp_factor: f32,

    #[serde(default = "def_false")]
    pub space_rotate_unlocked: bool,

    #[serde(default = "def_empty_vec_string")]
    pub alt_click_down: Vec<String>,

    #[serde(default = "def_empty_vec_string")]
    pub alt_click_up: Vec<String>,

    #[serde(default = "def_timezones")]
    pub timezones: Vec<String>,

    #[serde(default = "def_false")]
    pub clock_12h: bool,

    #[serde(default = "def_sets")]
    pub sets: Vec<SerializedWindowSet>,

    #[serde(default = "def_zero_u32")]
    pub last_set: u32,
}

impl GeneralConfig {
    fn sanitize_range(name: &str, val: f32, from: f32, to: f32) {
        assert!(
            !(!val.is_normal() || val < from || val > to),
            "GeneralConfig: {name} needs to be between {from} and {to}"
        );
    }

    pub fn load_from_disk() -> Self {
        let config = load_general();
        config.post_load();
        config
    }

    fn post_load(&self) {
        Self::sanitize_range("keyboard_scale", self.keyboard_scale, 0.05, 5.0);
        Self::sanitize_range("desktop_view_scale", self.desktop_view_scale, 0.05, 5.0);
        Self::sanitize_range("scroll_speed", self.scroll_speed, 0.01, 10.0);
    }
}

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

pub fn load_general() -> GeneralConfig {
    load_config_with_conf_d::<GeneralConfig>("config.yaml", config_io::ConfigRoot::Generic)
}

// Config that is saved from the settings panel

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
        last_set: config.last_set.clone(),
    };

    let json = serde_json::to_string_pretty(&conf).unwrap(); // want panic
    std::fs::write(get_state_path(), json)?;

    log::info!("State was saved successfully.");
    Ok(())
}
