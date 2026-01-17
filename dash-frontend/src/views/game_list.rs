use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
	widget::{
		ConstructEssentials,
		label::{WidgetLabel, WidgetLabelParams},
	},
};

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	util::{
		cached_fetcher::CoverArt,
		popup_manager::{MountPopupParams, PopupHandle},
		steam_utils::{self, AppID, SteamUtils},
		various::AsyncExecutor,
	},
	views::{self, game_cover, game_launcher},
};

#[derive(Clone)]
enum Task {
	AppManifestClicked(steam_utils::AppManifest),
	SetCoverArt(AppID, Rc<CoverArt>),
	CloseLauncher,
	Refresh,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub executor: AsyncExecutor,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

pub struct Cell {
	view_cover: game_cover::View,
}

struct State {
	view_launcher: Option<(PopupHandle, views::game_launcher::View)>,
}

pub struct View {
	#[allow(dead_code)]
	parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	id_list_parent: WidgetID,
	cells: HashMap<AppID, Cell>,
	game_cover_view_common: game_cover::ViewCommon,
	executor: AsyncExecutor,
	state: Rc<RefCell<State>>,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/game_list.xml"),
			extra: Default::default(),
		};

		let parser_state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;
		let list_parent = parser_state.fetch_widget(&params.layout.state, "list_parent")?;

		let tasks = Tasks::new();

		tasks.push(Task::Refresh);

		Ok(Self {
			parser_state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			globals: params.globals.clone(),
			id_list_parent: list_parent.id,
			cells: HashMap::new(),
			game_cover_view_common: game_cover::ViewCommon::new(params.globals.clone()),
			state: Rc::new(RefCell::new(State { view_launcher: None })),
			executor: params.executor,
		})
	}

	pub fn update(
		&mut self,
		layout: &mut Layout,
		steam_utils: &mut SteamUtils,
		executor: &AsyncExecutor,
	) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::Refresh => self.refresh(layout, steam_utils, executor)?,
					Task::AppManifestClicked(manifest) => self.action_app_manifest_clicked(manifest)?,
					Task::SetCoverArt(app_id, cover_art) => self.set_cover_art(layout, app_id, cover_art),
					Task::CloseLauncher => self.state.borrow_mut().view_launcher = None,
				}
			}
		}

		let mut state = self.state.borrow_mut();
		if let Some((_, view)) = &mut state.view_launcher {
			view.update(layout)?;
		}

		Ok(())
	}
}

pub struct Games {
	manifests: Vec<steam_utils::AppManifest>,
}

fn fill_game_list(
	ess: &mut ConstructEssentials,
	executor: &AsyncExecutor,
	cells: &mut HashMap<AppID, Cell>,
	games: &Games,
	tasks: &Tasks<Task>,
) -> anyhow::Result<()> {
	for manifest in &games.manifests {
		let on_loaded = {
			let app_id = manifest.app_id.clone();
			let tasks = tasks.clone();
			Box::new(move |cover_art: CoverArt| {
				tasks.push(Task::SetCoverArt(app_id, Rc::from(cover_art)));
			})
		};

		let view_cover = game_cover::View::new(game_cover::Params {
			ess,
			executor,
			manifest,
			on_loaded,
			scale: 1.0,
		})?;

		view_cover.button.on_click({
			let tasks = tasks.clone();
			let manifest = manifest.clone();
			Rc::new(move |_, _| {
				tasks.push(Task::AppManifestClicked(manifest.clone()));
				Ok(())
			})
		});

		cells.insert(manifest.app_id.clone(), Cell { view_cover });
	}

	Ok(())
}

impl View {
	fn game_list(&self, steam_utils: &mut SteamUtils) -> anyhow::Result<Games> {
		let manifests = steam_utils.list_installed_games(steam_utils::GameSortMethod::PlayDateDesc)?;

		Ok(Games { manifests })
	}

	fn refresh(
		&mut self,
		layout: &mut Layout,
		steam_utils: &mut SteamUtils,
		executor: &AsyncExecutor,
	) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);
		self.cells.clear();

		let mut text: Option<Translation> = None;
		match self.game_list(steam_utils) {
			Ok(list) => {
				if list.manifests.is_empty() {
					text = Some(Translation::from_translation_key("GAME_LIST.NO_GAMES_FOUND"))
				} else {
					fill_game_list(
						&mut ConstructEssentials {
							layout,
							parent: self.id_list_parent,
						},
						executor,
						&mut self.cells,
						&list,
						&self.tasks,
					)?
				}
			}
			Err(e) => text = Some(Translation::from_raw_text(&format!("Error: {:?}", e))),
		}

		if let Some(text) = text.take() {
			layout.add_child(
				self.id_list_parent,
				WidgetLabel::create(
					&mut self.globals.get(),
					WidgetLabelParams {
						content: text,
						..Default::default()
					},
				),
				Default::default(),
			)?;
		}

		Ok(())
	}

	fn set_cover_art(&mut self, layout: &mut Layout, app_id: AppID, cover_art: Rc<CoverArt>) {
		let Some(cell) = &mut self.cells.get_mut(&app_id) else {
			return;
		};

		if let Err(e) = cell
			.view_cover
			.set_cover_art(&mut self.game_cover_view_common, layout, &cover_art)
		{
			log::error!("{:?}", e);
		};
	}

	fn action_app_manifest_clicked(&mut self, manifest: steam_utils::AppManifest) -> anyhow::Result<()> {
		self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_raw_text(&manifest.name),
			on_content: {
				let state = self.state.clone();
				let tasks = self.tasks.clone();
				let executor = self.executor.clone();
				let globals = self.globals.clone();
				let frontend_tasks = self.frontend_tasks.clone();

				Rc::new(move |data| {
					let on_launched = {
						let tasks = tasks.clone();
						Box::new(move || tasks.push(Task::CloseLauncher))
					};

					let view = game_launcher::View::new(game_launcher::Params {
						manifest: manifest.clone(),
						executor: executor.clone(),
						globals: &globals,
						layout: data.layout,
						parent_id: data.id_content,
						frontend_tasks: &frontend_tasks,
						on_launched,
					})?;

					state.borrow_mut().view_launcher = Some((data.handle, view));
					Ok(())
				})
			},
		}));

		Ok(())
	}
}
