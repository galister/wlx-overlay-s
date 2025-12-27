use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::button::{ButtonClickCallback, ComponentButton},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{WidgetID, WidgetPair},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
};

use crate::{
	frontend::{Frontend, FrontendTask, FrontendTasks},
	tab::{Tab, TabType},
	util::{
		desktop_finder::DesktopEntry,
		popup_manager::{MountPopupParams, PopupHandle},
	},
	views::{self, app_launcher},
};

enum Task {
	CloseLauncher,
}

struct State {
	view_launcher: Option<(PopupHandle, views::app_launcher::View)>,
}

pub struct TabApps {
	#[allow(dead_code)]
	parser_state: ParserState,

	state: Rc<RefCell<State>>,
	entries: Vec<DesktopEntry>,
	app_list: AppList,
	tasks: Tasks<Task>,
}

impl Tab for TabApps {
	fn get_type(&self) -> TabType {
		TabType::Apps
	}

	fn update(&mut self, frontend: &mut Frontend) -> anyhow::Result<()> {
		let mut state = self.state.borrow_mut();

		for task in self.tasks.drain() {
			match task {
				Task::CloseLauncher => state.view_launcher = None,
			}
		}

		if let Some((_, launcher)) = &mut state.view_launcher {
			launcher.update(&mut frontend.layout, &mut frontend.interface)?;
		}
		Ok(())
	}
}

#[derive(Default)]
struct AppList {
	//data: Vec<ParserData>,
}

// called after the user clicks any desktop entry
fn on_app_click(
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	entry: DesktopEntry,
	state: Rc<RefCell<State>>,
	tasks: Tasks<Task>,
) -> ButtonClickCallback {
	Box::new(move |_common, _evt| {
		frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_raw_text(&entry.app_name),
			on_content: {
				// this is awful
				let state = state.clone();
				let entry = entry.clone();
				let globals = globals.clone();
				let frontend_tasks = frontend_tasks.clone();
				let tasks = tasks.clone();

				Rc::new(move |data| {
					let on_launched = {
						let tasks = tasks.clone();
						Box::new(move || tasks.push(Task::CloseLauncher))
					};

					let view = app_launcher::View::new(app_launcher::Params {
						entry: entry.clone(),
						globals: &globals,
						layout: data.layout,
						parent_id: data.id_content,
						frontend_tasks: &frontend_tasks,
						settings: data.settings,
						on_launched,
					})?;

					state.borrow_mut().view_launcher = Some((data.handle, view));
					Ok(())
				})
			},
		}));
		Ok(())
	})
}

impl TabApps {
	pub fn new(frontend: &mut Frontend, parent_id: WidgetID) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: frontend.layout.state.globals.clone(),
			path: AssetPath::BuiltIn("gui/tab/apps.xml"),
			extra: Default::default(),
		};

		let entries = frontend.desktop_finder.find_entries();

		let frontend_tasks = frontend.tasks.clone();
		let globals = frontend.layout.state.globals.clone();

		let tasks = Tasks::new();
		let state = Rc::new(RefCell::new(State { view_launcher: None }));

		let mut parser_state = wgui::parser::parse_from_assets(doc_params, &mut frontend.layout, parent_id)?;
		let app_list_parent = parser_state.fetch_widget(&frontend.layout.state, "app_list_parent")?;
		let mut app_list = AppList::default();
		app_list.mount_entries(
			frontend,
			&entries,
			&mut parser_state,
			doc_params,
			&app_list_parent,
			|button, entry| {
				// Set up the click handler for the app button
				button.on_click(on_app_click(
					frontend_tasks.clone(),
					globals.clone(),
					entry.clone(),
					state.clone(),
					tasks.clone(),
				));
			},
		)?;

		Ok(Self {
			app_list,
			parser_state,
			entries,
			state,
			tasks,
		})
	}
}

impl AppList {
	fn mount_entry(
		&mut self,
		frontend: &mut Frontend,
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
		list_parent: &WidgetPair,
		entry: &DesktopEntry,
	) -> anyhow::Result<Rc<ComponentButton>> {
		let mut template_params = HashMap::new();

		// entry icon
		template_params.insert(
			Rc::from("src_ext"),
			entry
				.icon_path
				.as_ref()
				.map_or_else(|| Rc::from(""), |icon_path| icon_path.clone()),
		);

		// entry fallback (question mark) icon
		template_params.insert(
			Rc::from("src"),
			if entry.icon_path.is_none() {
				Rc::from("dashboard/terminal.svg")
			} else {
				Rc::from("")
			},
		);

		template_params.insert(Rc::from("name"), entry.app_name.clone());

		let data = parser_state.parse_template(
			doc_params,
			"AppEntry",
			&mut frontend.layout,
			list_parent.id,
			template_params,
		)?;
		data.fetch_component_as::<ComponentButton>("button")
	}

	fn mount_entries(
		&mut self,
		frontend: &mut Frontend,
		entries: &[DesktopEntry],
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
		list_parent: &WidgetPair,
		on_button: impl Fn(Rc<ComponentButton>, &DesktopEntry),
	) -> anyhow::Result<()> {
		for entry in entries {
			let button = self.mount_entry(frontend, parser_state, doc_params, list_parent, entry)?;
			on_button(button, entry);
		}
		Ok(())
	}
}
