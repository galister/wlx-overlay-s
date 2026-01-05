use std::{
	cell::RefCell,
	collections::{HashMap, VecDeque},
	marker::PhantomData,
	rc::Rc,
};

use wgui::{
	assets::AssetPath,
	components::button::{ButtonClickCallback, ComponentButton},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{WidgetID, WidgetPair},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
};
use wlx_common::desktop_finder::DesktopEntry;

use crate::{
	frontend::{Frontend, FrontendTask, FrontendTasks},
	tab::{Tab, TabType},
	util::popup_manager::{MountPopupParams, PopupHandle},
	views::{self, app_launcher},
};

enum Task {
	CloseLauncher,
}

struct State {
	view_launcher: Option<(PopupHandle, views::app_launcher::View)>,
}

pub struct TabApps<T> {
	#[allow(dead_code)]
	parser_state: ParserState,

	state: Rc<RefCell<State>>,
	app_list: AppList,
	tasks: Tasks<Task>,
	marker: PhantomData<T>,
}

impl<T> Tab<T> for TabApps<T> {
	fn get_type(&self) -> TabType {
		TabType::Apps
	}

	fn update(&mut self, frontend: &mut Frontend<T>, data: &mut T) -> anyhow::Result<()> {
		let mut state = self.state.borrow_mut();

		for task in self.tasks.drain() {
			match task {
				Task::CloseLauncher => state.view_launcher = None,
			}
		}

		self
			.app_list
			.tick(frontend, &self.state, &self.tasks, &mut self.parser_state)?;

		if let Some((_, launcher)) = &mut state.view_launcher {
			launcher.update(&mut frontend.interface, data)?;
		}
		Ok(())
	}
}

struct AppList {
	//data: Vec<ParserData>,
	entries_to_mount: VecDeque<DesktopEntry>,
	list_parent: WidgetPair,
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

fn doc_params(globals: WguiGlobals) -> ParseDocumentParams<'static> {
	ParseDocumentParams {
		globals,
		path: AssetPath::BuiltIn("gui/tab/apps.xml"),
		extra: Default::default(),
	}
}

impl<T> TabApps<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID, data: &mut T) -> anyhow::Result<Self> {
		let globals = frontend.layout.state.globals.clone();
		let tasks = Tasks::new();
		let state = Rc::new(RefCell::new(State { view_launcher: None }));

		let mut entries = frontend.interface.desktop_finder(data).find_entries();
		let parser_state = wgui::parser::parse_from_assets(&doc_params(globals.clone()), &mut frontend.layout, parent_id)?;
		let app_list_parent = parser_state.fetch_widget(&frontend.layout.state, "app_list_parent")?;
		let app_list = AppList {
			entries_to_mount: entries.into_values().collect(),
			list_parent: app_list_parent,
		};

		Ok(Self {
			app_list,
			parser_state,
			state,
			tasks,
			marker: PhantomData,
		})
	}
}

impl AppList {
	fn mount_entry<T>(
		&mut self,
		frontend: &mut Frontend<T>,
		parser_state: &mut ParserState,
		doc_params: &ParseDocumentParams,
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
			self.list_parent.id,
			template_params,
		)?;
		data.fetch_component_as::<ComponentButton>("button")
	}

	fn tick<T>(
		&mut self,
		frontend: &mut Frontend<T>,
		state: &Rc<RefCell<State>>,
		tasks: &Tasks<Task>,
		parser_state: &mut ParserState,
	) -> anyhow::Result<()> {
		// load 4 entries for a single frame at most
		for _ in 0..4 {
			if let Some(entry) = self.entries_to_mount.pop_front() {
				let globals = frontend.layout.state.globals.clone();
				let button = self.mount_entry(frontend, parser_state, &doc_params(globals.clone()), &entry)?;

				button.on_click(on_app_click(
					frontend.tasks.clone(),
					globals.clone(),
					entry.clone(),
					state.clone(),
					tasks.clone(),
				));
			} else {
				break;
			}
		}

		Ok(())
	}
}
