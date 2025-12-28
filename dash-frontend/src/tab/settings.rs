use std::{marker::PhantomData, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::checkbox::ComponentCheckbox,
	layout::WidgetID,
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
};

use crate::{
	frontend::{Frontend, FrontendTask},
	settings,
	tab::{Tab, TabType},
};

enum Task {
	ToggleSetting(SettingType, bool),
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

	fn update(&mut self, frontend: &mut Frontend<T>, _data: &mut T) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::ToggleSetting(setting, n) => self.toggle_setting(frontend, setting, n),
			}
		}
		Ok(())
	}
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone)]
enum SettingType {
	DashHideUsername,
	DashAmPmClock,
	DashOpaqueBackground,
	DashXwaylandByDefault,
}

impl SettingType {
	fn get_bool<'a>(&self, settings: &'a mut settings::Settings) -> &'a mut bool {
		match self {
			SettingType::DashHideUsername => &mut settings.home_screen.hide_username,
			SettingType::DashAmPmClock => &mut settings.general.am_pm_clock,
			SettingType::DashOpaqueBackground => &mut settings.general.opaque_background,
			SettingType::DashXwaylandByDefault => &mut settings.tweaks.xwayland_by_default,
		}
	}
}

fn init_setting_checkbox<T>(
	frontend: &mut Frontend<T>,
	tasks: &Tasks<Task>,
	checkbox: Rc<ComponentCheckbox>,
	setting: SettingType,
	additional_frontend_task: Option<FrontendTask>,
) -> anyhow::Result<()> {
	let mut c = frontend.layout.start_common();
	checkbox.set_checked(&mut c.common(), *setting.get_bool(frontend.settings.get_mut()));

	let tasks = tasks.clone();
	let frontend_tasks = frontend.tasks.clone();

	checkbox.on_toggle(Box::new(move |_common, e| {
		tasks.push(Task::ToggleSetting(setting.clone(), e.checked));

		if let Some(task) = &additional_frontend_task {
			frontend_tasks.push(task.clone());
		}
		Ok(())
	}));

	c.finish()?;
	Ok(())
}

impl<T> TabSettings<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: frontend.layout.state.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/settings.xml"),
				extra: Default::default(),
			},
			&mut frontend.layout,
			parent_id,
		)?;

		let tasks = Tasks::new();

		init_setting_checkbox(
			frontend,
			&tasks,
			state.data.fetch_component_as::<ComponentCheckbox>("cb_hide_username")?,
			SettingType::DashHideUsername,
			None,
		)?;

		init_setting_checkbox(
			frontend,
			&tasks,
			state.data.fetch_component_as::<ComponentCheckbox>("cb_am_pm_clock")?,
			SettingType::DashAmPmClock,
			None,
		)?;

		init_setting_checkbox(
			frontend,
			&tasks,
			state
				.data
				.fetch_component_as::<ComponentCheckbox>("cb_opaque_background")?,
			SettingType::DashOpaqueBackground,
			Some(FrontendTask::RefreshBackground),
		)?;

		init_setting_checkbox(
			frontend,
			&tasks,
			state
				.data
				.fetch_component_as::<ComponentCheckbox>("cb_xwayland_by_default")?,
			SettingType::DashXwaylandByDefault,
			None,
		)?;

		Ok(Self {
			state,
			tasks,
			marker: PhantomData,
		})
	}

	fn toggle_setting(&mut self, frontend: &mut Frontend<T>, setting: SettingType, state: bool) {
		*setting.get_bool(frontend.settings.get_mut()) = state;
	}
}
