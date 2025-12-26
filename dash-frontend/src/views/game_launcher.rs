use crate::{
	frontend::{FrontendTask, FrontendTasks},
	util::{
		cached_fetcher::{self},
		steam_utils::{AppID, AppManifest},
		various::AsyncExecutor,
	},
};
use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
	widget::label::WidgetLabel,
};

#[derive(Clone)]
enum Task {
	FillAppDetails(cached_fetcher::AppDetailsJSONData),
	Launch,
}

pub struct Params<'a> {
	pub globals: &'a WguiGlobals,
	pub executor: AsyncExecutor,
	pub manifest: AppManifest,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub frontend_tasks: &'a FrontendTasks,
	pub on_launched: Box<dyn Fn()>,
}
pub struct View {
	#[allow(dead_code)]
	state: ParserState,
	tasks: Tasks<Task>,
	on_launched: Box<dyn Fn()>,
	frontend_tasks: FrontendTasks,

	#[allow(dead_code)]
	id_cover_art_parent: WidgetID,
	#[allow(dead_code)]
	executor: AsyncExecutor,
	#[allow(dead_code)]
	globals: WguiGlobals,
	#[allow(dead_code)]
	manifest: AppManifest,
}

impl View {
	async fn fetch_details(executor: AsyncExecutor, tasks: Tasks<Task>, app_id: AppID) {
		let Some(details) = cached_fetcher::get_app_details_json(executor, app_id).await else {
			return;
		};

		tasks.push(Task::FillAppDetails(details));
	}

	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/game_launcher.xml"),
			extra: Default::default(),
		};

		let state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let mut label_title = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_title")?;
		label_title.set_text_simple(
			&mut params.globals.get(),
			Translation::from_raw_text(&params.manifest.name),
		);

		let tasks = Tasks::new();

		// fetch details from the web
		let fut = View::fetch_details(params.executor.clone(), tasks.clone(), params.manifest.app_id.clone());
		params.executor.spawn(fut).detach();

		let id_cover_art_parent = state.get_widget_id("cover_art_parent")?;
		let btn_launch = state.fetch_component_as::<ComponentButton>("btn_launch")?;

		tasks.handle_button(&btn_launch, Task::Launch);

		Ok(Self {
			state,
			tasks,
			on_launched: params.on_launched,
			id_cover_art_parent,
			frontend_tasks: params.frontend_tasks.clone(),
			executor: params.executor.clone(),
			globals: params.globals.clone(),
			manifest: params.manifest,
		})
	}

	pub fn update(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::FillAppDetails(details) => self.action_fill_app_details(layout, details)?,
					Task::Launch => self.action_launch(),
				}
			}
		}

		Ok(())
	}

	fn action_fill_app_details(
		&mut self,
		layout: &mut Layout,
		mut details: cached_fetcher::AppDetailsJSONData,
	) -> anyhow::Result<()> {
		let mut c = layout.start_common();

		{
			let label_author = self.state.fetch_widget(&c.layout.state, "label_author")?.widget;
			let label_description = self.state.fetch_widget(&c.layout.state, "label_description")?.widget;

			if let Some(developer) = details.developers.pop() {
				label_author
					.cast::<WidgetLabel>()?
					.set_text(&mut c.common(), Translation::from_raw_text_string(developer));
			}

			let desc = if let Some(desc) = &details.short_description {
				Some(desc)
			} else if let Some(desc) = &details.detailed_description {
				Some(desc)
			} else {
				None
			};

			if let Some(desc) = desc {
				label_description
					.cast::<WidgetLabel>()?
					.set_text(&mut c.common(), Translation::from_raw_text(desc));
			}
		}

		c.finish()?;
		Ok(())
	}

	fn action_launch(&mut self) {
		self
			.frontend_tasks
			.push(FrontendTask::PushToast(Translation::from_raw_text("Game launch TODO")));
		(*self.on_launched)();
	}
}
