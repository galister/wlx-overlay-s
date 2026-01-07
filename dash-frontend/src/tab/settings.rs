use std::{collections::HashMap, marker::PhantomData, rc::Rc};

use strum::AsRefStr;
use wgui::{
	assets::AssetPath,
	components::{checkbox::ComponentCheckbox, slider::ComponentSlider},
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
};
use wlx_common::config::GeneralConfig;

use crate::{
	frontend::{Frontend, FrontendTask},
	tab::{Tab, TabType},
};

enum Task {
	UpdateBool(SettingType, bool),
	UpdateFloat(SettingType, f32),
	UpdateInt(SettingType, i32),
}

pub struct TabSettings<T> {
	#[allow(dead_code)]
	pub state: ParserState,

	tasks: Tasks<Task>,
	marker: PhantomData<T>,
}

impl<T> Tab<T> for TabSettings<T> {
	fn get_type(&self) -> TabType {
		TabType::Settings
	}

	fn update(&mut self, frontend: &mut Frontend<T>, data: &mut T) -> anyhow::Result<()> {
		let config = frontend.interface.general_config(data);
		let mut changed = false;
		for task in self.tasks.drain() {
			match task {
				Task::UpdateBool(setting, n) => {
					setting.get_frontend_task().map(|task| frontend.tasks.push(task));
					*setting.mut_bool(config) = n;
					changed = true;
				}
				Task::UpdateFloat(setting, n) => {
					setting.get_frontend_task().map(|task| frontend.tasks.push(task));
					*setting.mut_f32(config) = n;
					changed = true;
				}
				Task::UpdateInt(setting, n) => {
					setting.get_frontend_task().map(|task| frontend.tasks.push(task));
					*setting.mut_i32(config) = n;
					changed = true;
				}
			}
		}
		if changed {
			frontend.interface.config_changed(data);
		}
		Ok(())
	}
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, AsRefStr)]
enum SettingType {
	AnimationSpeed,
	RoundMultiplier,
	InvertScrollDirectionX,
	InvertScrollDirectionY,
	ScrollSpeed,
	LongPressDuration,
	NotificationsEnabled,
	NotificationsSoundEnabled,
	KeyboardSoundEnabled,
	UprightScreenFix,
	DoubleCursorFix,
	SetsOnWatch,
	HideGrabHelp,
	XrClickSensitivity,
	XrClickSensitivityRelease,
	AllowSliding,
	ClickFreezeTimeMs,
	FocusFollowsMouseMode,
	LeftHandedMouse,
	BlockGameInput,
	BlockGameInputIgnoreWatch,
	SpaceDragMultiplier,
	UseSkybox,
	UsePassthrough,
	ScreenRenderDown,
	PointerLerpFactor,
	SpaceDragUnlocked,
	SpaceRotateUnlocked,
	Clock12h,
	HideUsername,
	OpaqueBackground,
	XwaylandByDefault,
}

impl SettingType {
	pub fn mut_bool<'a>(self, config: &'a mut GeneralConfig) -> &'a mut bool {
		match self {
			Self::InvertScrollDirectionX => &mut config.invert_scroll_direction_x,
			Self::InvertScrollDirectionY => &mut config.invert_scroll_direction_y,
			Self::NotificationsEnabled => &mut config.notifications_enabled,
			Self::NotificationsSoundEnabled => &mut config.notifications_sound_enabled,
			Self::KeyboardSoundEnabled => &mut config.keyboard_sound_enabled,
			Self::UprightScreenFix => &mut config.upright_screen_fix,
			Self::DoubleCursorFix => &mut config.double_cursor_fix,
			Self::SetsOnWatch => &mut config.sets_on_watch,
			Self::HideGrabHelp => &mut config.hide_grab_help,
			Self::AllowSliding => &mut config.allow_sliding,
			Self::FocusFollowsMouseMode => &mut config.focus_follows_mouse_mode,
			Self::LeftHandedMouse => &mut config.left_handed_mouse,
			Self::BlockGameInput => &mut config.block_game_input,
			Self::BlockGameInputIgnoreWatch => &mut config.block_game_input_ignore_watch,
			Self::UseSkybox => &mut config.use_skybox,
			Self::UsePassthrough => &mut config.use_passthrough,
			Self::ScreenRenderDown => &mut config.screen_render_down,
			Self::SpaceDragUnlocked => &mut config.space_drag_unlocked,
			Self::SpaceRotateUnlocked => &mut config.space_rotate_unlocked,
			Self::Clock12h => &mut config.clock_12h,
			Self::HideUsername => &mut config.hide_username,
			Self::OpaqueBackground => &mut config.opaque_background,
			Self::XwaylandByDefault => &mut config.xwayland_by_default,
			_ => panic!("Requested bool for non-bool SettingType"),
		}
	}

	pub fn mut_f32<'a>(self, config: &'a mut GeneralConfig) -> &'a mut f32 {
		match self {
			Self::AnimationSpeed => &mut config.animation_speed,
			Self::RoundMultiplier => &mut config.round_multiplier,
			Self::ScrollSpeed => &mut config.scroll_speed,
			Self::LongPressDuration => &mut config.long_press_duration,
			Self::XrClickSensitivity => &mut config.xr_click_sensitivity,
			Self::XrClickSensitivityRelease => &mut config.xr_click_sensitivity_release,
			Self::SpaceDragMultiplier => &mut config.space_drag_multiplier,
			Self::PointerLerpFactor => &mut config.pointer_lerp_factor,
			_ => panic!("Requested f32 for non-f32 SettingType"),
		}
	}

	pub fn mut_i32<'a>(self, config: &'a mut GeneralConfig) -> &'a mut i32 {
		match self {
			Self::ClickFreezeTimeMs => &mut config.click_freeze_time_ms,
			_ => panic!("Requested i32 for non-i32 SettingType"),
		}
	}

	/// Ok is translation, Err is raw text
	fn get_translation(self) -> Result<&'static str, &'static str> {
		match self {
			Self::AnimationSpeed => Ok("APP_SETTINGS.ANIMATION_SPEED"),
			Self::RoundMultiplier => Ok("APP_SETTINGS.ROUND_MULTIPLIER"),
			Self::InvertScrollDirectionX => Ok("APP_SETTINGS.INVERT_SCROLL_DIRECTION_X"),
			Self::InvertScrollDirectionY => Ok("APP_SETTINGS.INVERT_SCROLL_DIRECTION_Y"),
			Self::ScrollSpeed => Ok("APP_SETTINGS.SCROLL_SPEED"),
			Self::LongPressDuration => Ok("APP_SETTINGS.LONG_PRESS_DURATION"),
			Self::NotificationsEnabled => Ok("APP_SETTINGS.NOTIFICATIONS_ENABLED"),
			Self::NotificationsSoundEnabled => Ok("APP_SETTINGS.NOTIFICATIONS_SOUND_ENABLED"),
			Self::KeyboardSoundEnabled => Ok("APP_SETTINGS.KEYBOARD_SOUND_ENABLED"),
			Self::UprightScreenFix => Ok("APP_SETTINGS.UPRIGHT_SCREEN_FIX"),
			Self::DoubleCursorFix => Ok("APP_SETTINGS.DOUBLE_CURSOR_FIX"),
			Self::SetsOnWatch => Ok("APP_SETTINGS.SETS_ON_WATCH"),
			Self::HideGrabHelp => Ok("APP_SETTINGS.HIDE_GRAB_HELP"),
			Self::XrClickSensitivity => Ok("APP_SETTINGS.XR_CLICK_SENSITIVITY"),
			Self::XrClickSensitivityRelease => Ok("APP_SETTINGS.XR_CLICK_SENSITIVITY_RELEASE"),
			Self::AllowSliding => Ok("APP_SETTINGS.ALLOW_SLIDING"),
			Self::ClickFreezeTimeMs => Ok("APP_SETTINGS.CLICK_FREEZE_TIME_MS"),
			Self::FocusFollowsMouseMode => Ok("APP_SETTINGS.FOCUS_FOLLOWS_MOUSE_MODE"),
			Self::LeftHandedMouse => Ok("APP_SETTINGS.LEFT_HANDED_MOUSE"),
			Self::BlockGameInput => Ok("APP_SETTINGS.BLOCK_GAME_INPUT"),
			Self::BlockGameInputIgnoreWatch => Ok("APP_SETTINGS.BLOCK_GAME_INPUT_IGNORE_WATCH"),
			Self::SpaceDragMultiplier => Ok("APP_SETTINGS.SPACE_DRAG_MULTIPLIER"),
			Self::UseSkybox => Ok("APP_SETTINGS.USE_SKYBOX"),
			Self::UsePassthrough => Ok("APP_SETTINGS.USE_PASSTHROUGH"),
			Self::ScreenRenderDown => Ok("APP_SETTINGS.SCREEN_RENDER_DOWN"),
			Self::PointerLerpFactor => Ok("APP_SETTINGS.POINTER_LERP_FACTOR"),
			Self::SpaceDragUnlocked => Ok("APP_SETTINGS.SPACE_DRAG_UNLOCKED"),
			Self::SpaceRotateUnlocked => Ok("APP_SETTINGS.SPACE_ROTATE_UNLOCKED"),
			Self::Clock12h => Ok("APP_SETTINGS.CLOCK_12H"),
			Self::HideUsername => Ok("APP_SETTINGS.HIDE_USERNAME"),
			Self::OpaqueBackground => Ok("APP_SETTINGS.OPAQUE_BACKGROUND"),
			Self::XwaylandByDefault => Ok("APP_SETTINGS.XWAYLAND_BY_DEFAULT"),
		}
	}

	fn get_tooltip(self) -> Option<&'static str> {
		match self {
			Self::UprightScreenFix => Some("APP_SETTINGS.UPRIGHT_SCREEN_FIX_HELP"),
			Self::DoubleCursorFix => Some("APP_SETTINGS.DOUBLE_CURSOR_FIX_HELP"),
			Self::XrClickSensitivity => Some("APP_SETTINGS.XR_CLICK_SENSITIVITY_HELP"),
			Self::XrClickSensitivityRelease => Some("APP_SETTINGS.XR_CLICK_SENSITIVITY_RELEASE_HELP"),
			Self::FocusFollowsMouseMode => Some("APP_SETTINGS.FOCUS_FOLLOWS_MOUSE_MODE_HELP"),
			Self::LeftHandedMouse => Some("APP_SETTINGS.LEFT_HANDED_MOUSE_HELP"),
			Self::BlockGameInput => Some("APP_SETTINGS.BLOCK_GAME_INPUT_HELP"),
			Self::BlockGameInputIgnoreWatch => Some("APP_SETTINGS.BLOCK_GAME_INPUT_IGNORE_WATCH_HELP"),
			Self::UseSkybox => Some("APP_SETTINGS.USE_SKYBOX_HELP"),
			Self::UsePassthrough => Some("APP_SETTINGS.USE_PASSTHROUGH_HELP"),
			Self::ScreenRenderDown => Some("APP_SETTINGS.SCREEN_RENDER_DOWN_HELP"),
			_ => None,
		}
	}

	//TODO: incorporate this
	fn requires_restart(self) -> bool {
		match self {
			Self::AnimationSpeed
			| Self::RoundMultiplier
			| Self::UprightScreenFix
			| Self::DoubleCursorFix
			| Self::SetsOnWatch
			| Self::UseSkybox
			| Self::UsePassthrough
			| Self::ScreenRenderDown => true,
			_ => false,
		}
	}

	fn get_frontend_task(self) -> Option<FrontendTask> {
		match self {
			Self::Clock12h => Some(FrontendTask::RefreshClock),
			Self::OpaqueBackground => Some(FrontendTask::RefreshBackground),
			_ => None,
		}
	}
}

macro_rules! category {
	($pe:expr, $root:expr, $translation:expr, $icon:expr) => {{
		let id = $pe.idx.to_string();
		$pe.idx += 1;

		let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
		params.insert(Rc::from("translation"), Rc::from($translation));
		params.insert(Rc::from("icon"), Rc::from($icon));
		params.insert(Rc::from("id"), Rc::from(id.as_ref()));

		$pe
			.parser_state
			.instantiate_template($pe.doc_params, "SettingsGroupBox", $pe.layout, $root, params)?;

		$pe.parser_state.get_widget_id(&id)
	}};
}

macro_rules! checkbox {
	($mp:expr, $root:expr, $setting:expr) => {
		let id = $mp.idx.to_string();
		$mp.idx += 1;

		let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
		params.insert(Rc::from("id"), Rc::from(id.as_ref()));

		match $setting.get_translation() {
			Ok(translation) => params.insert(Rc::from("translation"), translation.into()),
			Err(raw_text) => params.insert(Rc::from("text"), raw_text.into()),
		};

		if let Some(tooltip) = $setting.get_tooltip() {
			params.insert(Rc::from("tooltip"), Rc::from(tooltip));
		}

		let checked = if *$setting.mut_bool($mp.config) { "1" } else { "0" };
		params.insert(Rc::from("checked"), Rc::from(checked));

		$mp
			.parser_state
			.instantiate_template($mp.doc_params, "CheckBoxSetting", $mp.layout, $root, params)?;

		let checkbox = $mp.parser_state.fetch_component_as::<ComponentCheckbox>(&id)?;
		checkbox.on_toggle(Box::new({
			let tasks = $mp.tasks.clone();
			move |_common, e| {
				tasks.push(Task::UpdateBool($setting, e.checked));
				Ok(())
			}
		}));
	};
}

macro_rules! slider_f32 {
	($mp:expr, $root:expr, $setting:expr, $min:expr, $max:expr, $step:expr) => {
		let id = $mp.idx.to_string();
		$mp.idx += 1;

		let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
		params.insert(Rc::from("id"), Rc::from(id.as_ref()));

		match $setting.get_translation() {
			Ok(translation) => params.insert(Rc::from("translation"), translation.into()),
			Err(raw_text) => params.insert(Rc::from("text"), raw_text.into()),
		};

		if let Some(tooltip) = $setting.get_tooltip() {
			params.insert(Rc::from("tooltip"), Rc::from(tooltip));
		}

		let value = $setting.mut_f32($mp.config).to_string();
		params.insert(Rc::from("value"), Rc::from(value));
		params.insert(Rc::from("min"), Rc::from($min.to_string()));
		params.insert(Rc::from("max"), Rc::from($max.to_string()));
		params.insert(Rc::from("step"), Rc::from($step.to_string()));

		$mp
			.parser_state
			.instantiate_template($mp.doc_params, "SliderSetting", $mp.layout, $root, params)?;

		let slider = $mp.parser_state.fetch_component_as::<ComponentSlider>(&id)?;
		slider.on_value_changed(Box::new({
			let tasks = $mp.tasks.clone();
			move |_common, e| {
				tasks.push(Task::UpdateFloat($setting, e.value));
				Ok(())
			}
		}));
	};
}

macro_rules! slider_i32 {
	($mp:expr, $root:expr, $setting:expr, $min:expr, $max:expr, $step:expr) => {
		let id = $mp.idx.to_string();
		$mp.idx += 1;

		let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
		params.insert(Rc::from("id"), Rc::from(id.as_ref()));

		match $setting.get_translation() {
			Ok(translation) => params.insert(Rc::from("translation"), translation.into()),
			Err(raw_text) => params.insert(Rc::from("text"), raw_text.into()),
		};

		if let Some(tooltip) = $setting.get_tooltip() {
			params.insert(Rc::from("tooltip"), Rc::from(tooltip));
		}

		let value = $setting.mut_i32($mp.config).to_string();
		params.insert(Rc::from("value"), Rc::from(value));
		params.insert(Rc::from("min"), Rc::from($min.to_string()));
		params.insert(Rc::from("max"), Rc::from($max.to_string()));
		params.insert(Rc::from("step"), Rc::from($step.to_string()));

		$mp
			.parser_state
			.instantiate_template($mp.doc_params, "SliderSetting", $mp.layout, $root, params)?;

		let slider = $mp.parser_state.fetch_component_as::<ComponentSlider>(&id)?;
		slider.on_value_changed(Box::new({
			let tasks = $mp.tasks.clone();
			move |_common, e| {
				tasks.push(Task::UpdateInt($setting, e.value as i32));
				Ok(())
			}
		}));
	};
}

struct MacroParams<'a> {
	layout: &'a mut Layout,
	parser_state: &'a mut ParserState,
	doc_params: &'a ParseDocumentParams<'a>,
	config: &'a mut GeneralConfig,
	tasks: Tasks<Task>,
	idx: usize,
}

impl<T> TabSettings<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID, data: &mut T) -> anyhow::Result<Self> {
		let doc_params = ParseDocumentParams {
			globals: frontend.layout.state.globals.clone(),
			path: AssetPath::BuiltIn("gui/tab/settings.xml"),
			extra: Default::default(),
		};
		let mut parser_state = wgui::parser::parse_from_assets(&doc_params, &mut frontend.layout, parent_id)?;

		let root = parser_state.get_widget_id("settings_root")?;

		let mut mp = MacroParams {
			layout: &mut frontend.layout,
			parser_state: &mut parser_state,
			doc_params: &doc_params,
			config: frontend.interface.general_config(data),
			tasks: Tasks::default(),
			idx: 9001,
		};

		let c = category!(mp, root, "APP_SETTINGS.LOOK_AND_FEEL", "dashboard/palette.svg")?;
		checkbox!(mp, c, SettingType::OpaqueBackground);
		checkbox!(mp, c, SettingType::HideUsername);
		checkbox!(mp, c, SettingType::HideGrabHelp);
		slider_f32!(mp, c, SettingType::AnimationSpeed, 0.5, 5.0, 0.1); // min, max, step
		slider_f32!(mp, c, SettingType::RoundMultiplier, 0.5, 5.0, 0.1);
		checkbox!(mp, c, SettingType::SetsOnWatch);
		checkbox!(mp, c, SettingType::UseSkybox);
		checkbox!(mp, c, SettingType::UsePassthrough);
		checkbox!(mp, c, SettingType::Clock12h);

		let c = category!(mp, root, "APP_SETTINGS.FEATURES", "dashboard/options.svg")?;
		checkbox!(mp, c, SettingType::NotificationsEnabled);
		checkbox!(mp, c, SettingType::NotificationsSoundEnabled);
		checkbox!(mp, c, SettingType::KeyboardSoundEnabled);
		checkbox!(mp, c, SettingType::SpaceDragUnlocked);
		checkbox!(mp, c, SettingType::SpaceRotateUnlocked);
		slider_f32!(mp, c, SettingType::SpaceDragMultiplier, -10.0, 10.0, 0.5);
		checkbox!(mp, c, SettingType::BlockGameInput);
		checkbox!(mp, c, SettingType::BlockGameInputIgnoreWatch);

		let c = category!(mp, root, "APP_SETTINGS.CONTROLS", "dashboard/controller.svg")?;
		checkbox!(mp, c, SettingType::FocusFollowsMouseMode);
		checkbox!(mp, c, SettingType::LeftHandedMouse);
		checkbox!(mp, c, SettingType::AllowSliding);
		checkbox!(mp, c, SettingType::InvertScrollDirectionX);
		checkbox!(mp, c, SettingType::InvertScrollDirectionY);
		slider_f32!(mp, c, SettingType::ScrollSpeed, 0.1, 5.0, 0.1);
		slider_f32!(mp, c, SettingType::LongPressDuration, 0.1, 2.0, 0.1);
		slider_f32!(mp, c, SettingType::PointerLerpFactor, 0.1, 1.0, 0.1);
		slider_f32!(mp, c, SettingType::XrClickSensitivity, 0.1, 1.0, 0.1);
		slider_f32!(mp, c, SettingType::XrClickSensitivityRelease, 0.1, 1.0, 0.1);
		slider_i32!(mp, c, SettingType::ClickFreezeTimeMs, 0, 500, 50);

		let c = category!(mp, root, "APP_SETTINGS.MISC", "dashboard/blocks.svg")?;
		checkbox!(mp, c, SettingType::XwaylandByDefault);
		checkbox!(mp, c, SettingType::UprightScreenFix);
		checkbox!(mp, c, SettingType::DoubleCursorFix);
		checkbox!(mp, c, SettingType::ScreenRenderDown);

		Ok(Self {
			tasks: mp.tasks,
			state: parser_state,
			marker: PhantomData,
		})
	}
}
