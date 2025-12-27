use std::{collections::HashMap, rc::Rc};

use wayvr_ipc::packet_client::WvrProcessLaunchParams;
use wgui::{
	assets::AssetPath,
	components::{button::ComponentButton, checkbox::ComponentCheckbox},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
	widget::label::WidgetLabel,
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	settings::SettingsIO,
	util::desktop_finder::DesktopEntry,
};

#[derive(Clone, Eq, PartialEq)]
enum RunMode {
	Cage,
	Wayland,
}

#[derive(Clone)]
enum Task {
	SetRunMode(RunMode),
	Launch,
}

struct LaunchParams<'a> {
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
		let btn_launch = state.fetch_component_as::<ComponentButton>("btn_launch")?;

		{
			let mut label_exec = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_exec")?;
			let mut label_args = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_args")?;

			label_exec.set_text_simple(
				&mut params.globals.get(),
				Translation::from_raw_text_rc(params.entry.app_name.clone()),
			);

			label_args.set_text_simple(
				&mut params.globals.get(),
				Translation::from_raw_text_rc(params.entry.exec_args.clone()),
			);
		}

		let tasks = Tasks::new();

		tasks.handle_button(&btn_launch, Task::Launch);

		let id_icon_parent = state.get_widget_id("icon_parent")?;

		// app icon
		if let Some(icon_path) = &params.entry.icon_path {
			let mut template_params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
			template_params.insert("path".into(), icon_path.clone());
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
					Task::Launch => self.action_launch(interface),
				}
			}
		}

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

	fn action_launch(&mut self, interface: &mut BoxDashInterface) {
		View::try_launch(LaunchParams {
			application: &self.entry,
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

		let args = match params.run_mode {
			RunMode::Cage => format!("-- {} {}", params.application.exec_path, params.application.exec_args),
			RunMode::Wayland => params.application.exec_args.to_string(),
		};

		let exec = match params.run_mode {
			RunMode::Cage => "cage".to_string(),
			RunMode::Wayland => params.application.exec_path.to_string(),
		};

		let mut userdata = HashMap::new();
		userdata.insert("desktop-entry".to_string(), serde_json::to_string(params.application)?);

		params.interface.process_launch(WvrProcessLaunchParams {
			env,
			exec,
			name: params.application.app_name.to_string(),
			args,
			userdata,
		})?;

		params
			.frontend_tasks
			.push(FrontendTask::PushToast(Translation::from_translation_key(
				"APPLICATION_STARTED",
			)));

		(*params.on_launched)();

		// we're done!
		Ok(())
	}
}
