use std::rc::Rc;

use wgui::{
	assets::AssetPath,
	components::checkbox::ComponentCheckbox,
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{
	frontend::{Frontend, FrontendTask},
	settings,
	tab::{Tab, TabParams, TabType},
};

pub struct TabSettings {
	#[allow(dead_code)]
	pub state: ParserState,
}

impl Tab for TabSettings {
	fn get_type(&self) -> TabType {
		TabType::Settings
	}
}

fn init_setting_checkbox(
	params: &mut TabParams,
	checkbox: Rc<ComponentCheckbox>,
	fetch_callback: fn(&mut settings::Settings) -> &mut bool,
	change_callback: Option<fn(&mut Frontend, bool)>,
) -> anyhow::Result<()> {
	let mut c = params.layout.start_common();

	checkbox.set_checked(&mut c.common(), *fetch_callback(params.settings));
	let rc_frontend = params.frontend.clone();
	checkbox.on_toggle(Box::new(move |_common, e| {
		let mut frontend = rc_frontend.borrow_mut();
		*fetch_callback(frontend.settings.get_mut()) = e.checked;

		if let Some(change_callback) = &change_callback {
			change_callback(&mut frontend, e.checked);
		}

		frontend.settings.mark_as_dirty();

		Ok(())
	}));

	c.finish()?;
	Ok(())
}

impl TabSettings {
	pub fn new(mut params: TabParams) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals: params.globals.clone(),
				path: AssetPath::BuiltIn("gui/tab/settings.xml"),
				extra: Default::default(),
			},
			params.layout,
			params.parent_id,
		)?;

		init_setting_checkbox(
			&mut params,
			state.data.fetch_component_as::<ComponentCheckbox>("cb_hide_username")?,
			|settings| &mut settings.home_screen.hide_username,
			None,
		)?;

		init_setting_checkbox(
			&mut params,
			state.data.fetch_component_as::<ComponentCheckbox>("cb_am_pm_clock")?,
			|settings| &mut settings.general.am_pm_clock,
			Some(|frontend, _| {
				frontend.tasks.push(FrontendTask::RefreshClock);
			}),
		)?;

		init_setting_checkbox(
			&mut params,
			state
				.data
				.fetch_component_as::<ComponentCheckbox>("cb_opaque_background")?,
			|settings| &mut settings.general.opaque_background,
			Some(|frontend, _| {
				frontend.tasks.push(FrontendTask::RefreshBackground);
			}),
		)?;

		init_setting_checkbox(
			&mut params,
			state
				.data
				.fetch_component_as::<ComponentCheckbox>("cb_xwayland_by_default")?,
			|settings| &mut settings.tweaks.xwayland_by_default,
			None,
		)?;

		Ok(Self { state })
	}
}
