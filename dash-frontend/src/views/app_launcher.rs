use std::{collections::HashMap, rc::Rc};

use anyhow::Context;
use wayvr_ipc::{packet_client::WvrProcessLaunchParams, packet_server::WvrDisplayHandle};
use wgui::{
	assets::AssetPath,
	components::checkbox::ComponentCheckbox,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::label::WidgetLabel,
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	settings::SettingsIO,
	task::Tasks,
	util::desktop_finder::DesktopEntry,
	views::display_list,
};

#[derive(Clone, Eq, PartialEq)]
enum RunMode {
	Cage,
	Wayland,
}

enum Task {
	SetRunMode(RunMode),
	DisplayClick(WvrDisplayHandle),
}

struct LaunchParams<'a> {
	display_handle: WvrDisplayHandle,
	application: &'a DesktopEntry,
	run_mode: RunMode,
	globals: &'a WguiGlobals,
	frontend_tasks: &'a FrontendTasks,
	interface: &'a mut BoxDashInterface,
	on_launched: &'a dyn Fn(),
}

pub struct View {
	#[allow(dead_code)]
	state: ParserState,
	entry: DesktopEntry,
	view_display_list: display_list::View,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,

	cb_cage_mode: Rc<ComponentCheckbox>,
	cb_wayland_mode: Rc<ComponentCheckbox>,
	run_mode: RunMode,

	on_launched: Box<dyn Fn()>,
}

pub struct Params<'a> {
	pub globals: &'a WguiGlobals,
	pub entry: DesktopEntry,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub settings: &'a dyn SettingsIO,
	pub frontend_tasks: &'a FrontendTasks,
	pub on_launched: Box<dyn Fn()>,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/app_launcher.xml"),
			extra: Default::default(),
		};

		let mut state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let cb_cage_mode = state.fetch_component_as::<ComponentCheckbox>("cb_cage_mode")?;
		let cb_wayland_mode = state.fetch_component_as::<ComponentCheckbox>("cb_wayland_mode")?;

		{
			let mut label_exec = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_exec")?;
			let mut label_args = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_args")?;

			label_exec.set_text_simple(
				&mut params.globals.get(),
				Translation::from_raw_text_string(params.entry.app_name.clone()),
			);

			label_args.set_text_simple(
				&mut params.globals.get(),
				Translation::from_raw_text_string(params.entry.exec_args.join(" ")),
			);
		}

		let display_list_parent = state.fetch_widget(&params.layout.state, "display_list_parent")?.id;

		let tasks = Tasks::new();

		let on_display_click = {
			let tasks = tasks.clone();
			Box::new(move |disp_handle: WvrDisplayHandle| {
				tasks.push(Task::DisplayClick(disp_handle));
			})
		};

		let view_display_list = display_list::View::new(display_list::Params {
			frontend_tasks: params.frontend_tasks.clone(),
			globals: params.globals,
			layout: params.layout,
			parent_id: display_list_parent,
			on_click: Some(on_display_click),
		})?;

		let id_icon_parent = state.get_widget_id("icon_parent")?;

		// app icon
		if let Some(icon_path) = &params.entry.icon_path {
			let mut template_params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
			template_params.insert("path".into(), icon_path.as_str().into());
			state.instantiate_template(
				doc_params,
				"ApplicationIcon",
				params.layout,
				id_icon_parent,
				template_params,
			)?;
		}

		let run_mode = if params.settings.get().tweaks.xwayland_by_default {
			RunMode::Cage
		} else {
			RunMode::Wayland
		};

		tasks.push(Task::SetRunMode(run_mode.clone()));

		cb_cage_mode.on_toggle({
			let tasks = tasks.clone();
			Box::new(move |_, _| {
				tasks.push(Task::SetRunMode(RunMode::Cage));
				Ok(())
			})
		});

		cb_wayland_mode.on_toggle({
			let tasks = tasks.clone();
			Box::new(move |_, _| {
				tasks.push(Task::SetRunMode(RunMode::Wayland));
				Ok(())
			})
		});

		let mut label_title = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_title")?;

		label_title.set_text_simple(
			&mut params.globals.get(),
			Translation::from_raw_text(&params.entry.app_name),
		);

		Ok(Self {
			state,
			view_display_list,
			tasks,
			cb_cage_mode,
			cb_wayland_mode,
			run_mode,
			entry: params.entry,
			frontend_tasks: params.frontend_tasks.clone(),
			globals: params.globals.clone(),
			on_launched: params.on_launched,
		})
	}

	pub fn update(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::SetRunMode(run_mode) => self.action_set_run_mode(layout, run_mode)?,
					Task::DisplayClick(disp_handle) => self.action_display_click(disp_handle, interface),
				}
			}
		}

		self.view_display_list.update(layout, interface)?;

		Ok(())
	}

	fn action_set_run_mode(&mut self, layout: &mut Layout, run_mode: RunMode) -> anyhow::Result<()> {
		let (n1, n2) = match run_mode {
			RunMode::Cage => (true, false),
			RunMode::Wayland => (false, true),
		};

		let mut c = layout.start_common();
		self.cb_cage_mode.set_checked(&mut c.common(), n1);
		self.cb_wayland_mode.set_checked(&mut c.common(), n2);

		c.finish()?;
		Ok(())
	}

	fn action_display_click(&mut self, handle: WvrDisplayHandle, interface: &mut BoxDashInterface) {
		View::try_launch(LaunchParams {
			application: &self.entry,
			display_handle: handle,
			frontend_tasks: &self.frontend_tasks,
			globals: &self.globals,
			run_mode: self.run_mode.clone(),
			interface,
			on_launched: &self.on_launched,
		});
	}

	fn try_launch(params: LaunchParams) {
		let globals = params.globals.clone();
		let frontend_tasks = params.frontend_tasks.clone();

		// launch app itself
		let Err(e) = View::launch(params) else { return };

		let str_failed = globals.i18n().translate("FAILED_TO_LAUNCH_APPLICATION");
		frontend_tasks.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
			"{} {:?}",
			str_failed, e
		))));
	}

	fn launch(params: LaunchParams) -> anyhow::Result<()> {
		let mut env = Vec::<String>::new();

		if params.run_mode == RunMode::Wayland {
			// This list could be larger, feel free to expand it
			env.push("QT_QPA_PLATFORM=wayland".into());
			env.push("GDK_BACKEND=wayland".into());
			env.push("SDL_VIDEODRIVER=wayland".into());
			env.push("XDG_SESSION_TYPE=wayland".into());
			env.push("ELECTRON_OZONE_PLATFORM_HINT=wayland".into());
		}

		// TODO: refactor this after we ditch old wayvr-dashboard completely
		let desktop_file = params.application.to_desktop_file();
		let mut userdata = HashMap::<String, String>::new();
		userdata.insert("desktop_file".into(), serde_json::to_string(&desktop_file)?);

		let exec_args_str = desktop_file.exec_args.join(" ");

		params
			.interface
			.display_set_visible(params.display_handle.clone(), true)?;

		let args = match params.run_mode {
			RunMode::Cage => format!("-- {} {}", desktop_file.exec_path, exec_args_str),
			RunMode::Wayland => exec_args_str,
		};

		let exec = match params.run_mode {
			RunMode::Cage => "cage",
			RunMode::Wayland => &desktop_file.name,
		};

		let display = params
			.interface
			.display_get(params.display_handle.clone())
			.context("Display not found")?;

		params.interface.process_launch(WvrProcessLaunchParams {
			env,
			exec: String::from(exec),
			name: desktop_file.name,
			target_display: params.display_handle,
			args,
			userdata,
		})?;

		let str_launched_on = params
			.globals
			.i18n()
			.translate_and_replace("APPLICATION_LAUNCHED_ON", ("{DISPLAY_NAME}", &display.name));

		params
			.frontend_tasks
			.push(FrontendTask::PushToast(Translation::from_raw_text_string(
				str_launched_on,
			)));

		(*params.on_launched)();

		// we're done!
		Ok(())
	}
}
