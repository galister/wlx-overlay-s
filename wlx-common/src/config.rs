use std::{collections::HashMap, sync::Arc};

use chrono::Offset;
use glam::Affine3A;
use idmap::IdMap;
use serde::{Deserialize, Serialize};

use crate::{
	astr_containers::{AStrMap, AStrSet},
	overlays::{ToastDisplayMethod, ToastTopic},
	windowing::OverlayWindowState,
};

pub type PwTokenMap = AStrMap<String>;
pub type SerializedWindowStates = HashMap<Arc<str>, OverlayWindowState>;

#[derive(Clone, Serialize, Deserialize)]
pub struct SerializedWindowSet {
	pub name: Arc<str>,
	pub overlays: SerializedWindowStates,
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

const fn def_true() -> bool {
	true
}

const fn def_false() -> bool {
	false
}

const fn def_one() -> f32 {
	1.0
}

const fn def_half() -> f32 {
	0.5
}

const fn def_point7() -> f32 {
	0.7
}

const fn def_point3() -> f32 {
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

fn def_global_set() -> SerializedWindowStates {
	HashMap::new()
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

const fn def_astrset_empty() -> AStrSet {
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

fn def_theme_path() -> Arc<str> {
	"theme".into()
}

fn def_toast_topics() -> IdMap<ToastTopic, ToastDisplayMethod> {
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
	#[serde(default = "def_theme_path")]
	pub theme_path: Arc<str>,

	pub color_accent: Option<String>,
	pub color_danger: Option<String>,
	pub color_faded: Option<String>,

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
	pub notification_topics: IdMap<ToastTopic, ToastDisplayMethod>,

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

	#[serde(default = "def_false")]
	pub single_set_mode: bool,

	#[serde(default = "def_astrset_empty")]
	pub custom_panels: AStrSet,

	#[serde(default = "def_astrset_empty")]
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
	pub left_handed_mouse: bool,

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

	#[serde(default = "def_global_set")]
	pub global_set: SerializedWindowStates,

	#[serde(default = "def_zero_u32")]
	pub last_set: u32,
}
