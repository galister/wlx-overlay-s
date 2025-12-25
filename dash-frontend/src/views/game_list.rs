use std::{cell::RefCell, rc::Rc};

use wayvr_ipc::packet_server::{self, WvrWindowHandle};
use wgui::{
	assets::AssetPath,
	components::{
		self,
		button::ComponentButton,
		tooltip::{TooltipInfo, TooltipSide},
	},
	drawing::{self, GradientMode},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID, WidgetPair},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	taffy::{
		self,
		prelude::{length, percent},
	},
	widget::{
		ConstructEssentials,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle,
		util::WLength,
	},
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	task::Tasks,
	util::{
		popup_manager::MountPopupParams,
		steam_utils::{self, SteamUtils},
	},
	views::window_options,
};

#[derive(Clone)]
enum Task {
	AppManifestClicked(steam_utils::AppManifest),
	Refresh,
}

pub struct Params<'a> {
	pub globals: &'a WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	id_list_parent: WidgetID,
	steam_utils: steam_utils::SteamUtils,
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

		let steam_utils = SteamUtils::new()?;

		tasks.push(Task::Refresh);

		Ok(Self {
			parser_state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			globals: params.globals.clone(),
			id_list_parent: list_parent.id,
			steam_utils,
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
					Task::Refresh => self.refresh(layout, interface)?,
					Task::AppManifestClicked(manifest) => self.action_app_manifest_clicked(manifest)?,
				}
			}
		}

		Ok(())
	}
}

pub struct Games {
	manifests: Vec<steam_utils::AppManifest>,
}

const BORDER_COLOR_DEFAULT: drawing::Color = drawing::Color::new(1.0, 1.0, 1.0, 0.3);
const BORDER_COLOR_HOVERED: drawing::Color = drawing::Color::new(1.0, 1.0, 1.0, 1.0);

const GAME_COVER_SIZE_X: f32 = 140.0;
const GAME_COVER_SIZE_Y: f32 = 210.0;

pub fn construct_game_cover(
	ess: &mut ConstructEssentials,
	globals: &WguiGlobals,
	manifest: &steam_utils::AppManifest,
) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>)> {
	let (widget_button, button) = components::button::construct(
		ess,
		components::button::Params {
			color: Some(drawing::Color::new(1.0, 1.0, 1.0, 0.1)),
			border_color: Some(BORDER_COLOR_DEFAULT),
			hover_border_color: Some(BORDER_COLOR_HOVERED),
			round: WLength::Units(12.0),
			border: 2.0,
			tooltip: Some(TooltipInfo {
				side: TooltipSide::Bottom,
				text: Translation::from_raw_text(&manifest.name),
			}),
			style: taffy::Style {
				position: taffy::Position::Relative,
				align_items: Some(taffy::AlignItems::Center),
				justify_content: Some(taffy::JustifyContent::Center),
				size: taffy::Size {
					width: length(GAME_COVER_SIZE_X),
					height: length(GAME_COVER_SIZE_Y),
				},
				..Default::default()
			},
			..Default::default()
		},
	)?;

	let rect_gradient = |color: drawing::Color, color2: drawing::Color| {
		rectangle::WidgetRectangle::create(rectangle::WidgetRectangleParams {
			color,
			color2,
			round: WLength::Units(12.0),
			gradient: GradientMode::Vertical,
			..Default::default()
		})
	};

	let rect_gradient_style = |align_self: taffy::AlignSelf, height: f32| taffy::Style {
		position: taffy::Position::Absolute,
		align_self: Some(align_self),
		size: taffy::Size {
			width: percent(1.0),
			height: percent(height),
		},
		..Default::default()
	};

	// top shine
	ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(1.0, 1.0, 1.0, 0.25),
			drawing::Color::new(1.0, 1.0, 1.0, 0.0),
		),
		rect_gradient_style(taffy::AlignSelf::Baseline, 0.05),
	)?;

	// top white gradient
	ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(1.0, 1.0, 1.0, 0.2),
			drawing::Color::new(1.0, 1.0, 1.0, 0.0),
		),
		rect_gradient_style(taffy::AlignSelf::Baseline, 0.5),
	)?;

	// bottom black gradient
	ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(0.0, 0.0, 0.0, 0.0),
			drawing::Color::new(0.0, 0.0, 0.0, 0.2),
		),
		rect_gradient_style(taffy::AlignSelf::End, 0.5),
	)?;

	// bottom shadow
	ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(0.0, 0.0, 0.0, 0.0),
			drawing::Color::new(0.0, 0.0, 0.0, 0.2),
		),
		rect_gradient_style(taffy::AlignSelf::End, 0.05),
	)?;

	Ok((widget_button, button))
}

fn fill_game_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	interface: &mut BoxDashInterface,
	games: &Games,
	tasks: &Tasks<Task>,
) -> anyhow::Result<()> {
	for manifest in &games.manifests {
		let (_, button) = construct_game_cover(ess, globals, manifest)?;

		button.on_click({
			let tasks = tasks.clone();
			let manifest = manifest.clone();
			Box::new(move |_, _| {
				tasks.push(Task::AppManifestClicked(manifest.clone()));
				Ok(())
			})
		});
	}

	Ok(())
}

impl View {
	fn game_list(&self) -> anyhow::Result<Games> {
		let manifests = self
			.steam_utils
			.list_installed_games(steam_utils::GameSortMethod::PlayDateDesc)?;

		Ok(Games { manifests })
	}

	fn refresh(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);

		let mut text: Option<Translation> = None;
		match self.game_list() {
			Ok(list) => {
				if list.manifests.is_empty() {
					text = Some(Translation::from_translation_key("GAME_LIST.NO_GAMES_FOUND"))
				} else {
					fill_game_list(
						&self.globals,
						&mut ConstructEssentials {
							layout,
							parent: self.id_list_parent,
						},
						interface,
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

	fn action_app_manifest_clicked(&mut self, manifest: steam_utils::AppManifest) -> anyhow::Result<()> {
		self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_raw_text(&manifest.name),
			on_content: {
				let frontend_tasks = self.frontend_tasks.clone();
				let globals = self.globals.clone();
				let tasks = self.tasks.clone();

				Rc::new(move |data| {
					// todo
					Ok(())
				})
			},
		}));

		Ok(())
	}
}
