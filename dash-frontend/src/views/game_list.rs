use std::{collections::HashMap, rc::Rc};

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
	renderer_vk::text::{
		FontWeight, HorizontalAlign, TextShadow, TextStyle,
		custom_glyph::{CustomGlyphContent, CustomGlyphData},
	},
	taffy::{
		self, AlignItems, AlignSelf, JustifyContent, JustifySelf,
		prelude::{auto, length, percent},
	},
	task::Tasks,
	widget::{
		ConstructEssentials,
		div::WidgetDiv,
		image::{WidgetImage, WidgetImageParams},
		label::{WidgetLabel, WidgetLabelParams},
		rectangle,
		util::WLength,
	},
};

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	util::{
		cover_art_fetcher::{self, CoverArt},
		popup_manager::MountPopupParams,
		steam_utils::{self, AppID, AppManifest, SteamUtils},
		various::AsyncExecutor,
	},
};

#[derive(Clone)]
enum Task {
	AppManifestClicked(steam_utils::AppManifest),
	SetCoverArt((AppID, Rc<CoverArt>)),
	Refresh,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

struct Cell {
	image_parent: WidgetID,
	manifest: AppManifest,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	id_list_parent: WidgetID,
	steam_utils: steam_utils::SteamUtils,

	cells: HashMap<AppID, Cell>,
	img_placeholder: Option<CustomGlyphData>,
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
			cells: HashMap::new(),
			img_placeholder: None,
		})
	}

	pub fn update(&mut self, layout: &mut Layout, executor: &AsyncExecutor) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::Refresh => self.refresh(layout, executor)?,
					Task::AppManifestClicked(manifest) => self.action_app_manifest_clicked(manifest)?,
					Task::SetCoverArt((app_id, cover_art)) => self.action_set_cover_art(layout, &app_id, cover_art)?,
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

async fn request_cover_image(executor: AsyncExecutor, manifest: steam_utils::AppManifest, tasks: Tasks<Task>) {
	let cover_art = match cover_art_fetcher::request_image(executor, manifest.app_id.clone()).await {
		Ok(cover_art) => cover_art,
		Err(e) => {
			log::error!("request_cover_image failed: {:?}", e);
			return;
		}
	};

	tasks.push(Task::SetCoverArt((manifest.app_id, Rc::from(cover_art))));
}

fn construct_game_cover(
	ess: &mut ConstructEssentials,
	executor: &AsyncExecutor,
	tasks: &Tasks<Task>,
	_globals: &WguiGlobals,
	manifest: &steam_utils::AppManifest,
) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>, Cell)> {
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

	let (image_parent, _) = ess.layout.add_child(
		widget_button.id,
		WidgetDiv::create(),
		taffy::Style {
			position: taffy::Position::Absolute,
			size: taffy::Size {
				width: percent(1.0),
				height: percent(1.0),
			},
			padding: taffy::Rect::length(2.0),
			align_items: Some(AlignItems::Center),
			justify_content: Some(JustifyContent::Center),
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
	let (top_shine, _) = ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(1.0, 1.0, 1.0, 0.25),
			drawing::Color::new(1.0, 1.0, 1.0, 0.0),
		),
		rect_gradient_style(taffy::AlignSelf::Baseline, 0.05),
	)?;

	// not optimal, this forces us to create a new pass for every created cover art just to overlay various rectangles at the top of the image cover art
	top_shine.widget.state().flags.new_pass = true;

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
			drawing::Color::new(0.0, 0.0, 0.0, 0.25),
		),
		rect_gradient_style(taffy::AlignSelf::End, 0.5),
	)?;

	// bottom shadow
	ess.layout.add_child(
		widget_button.id,
		rect_gradient(
			drawing::Color::new(0.0, 0.0, 0.0, 0.0),
			drawing::Color::new(0.0, 0.0, 0.0, 0.5),
		),
		rect_gradient_style(taffy::AlignSelf::End, 0.1),
	)?;

	// request cover image data from the internet or disk cache
	executor
		.spawn(request_cover_image(executor.clone(), manifest.clone(), tasks.clone()))
		.detach();

	Ok((
		widget_button,
		button,
		Cell {
			image_parent: image_parent.id,
			manifest: manifest.clone(),
		},
	))
}

fn fill_game_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	executor: &AsyncExecutor,
	cells: &mut HashMap<AppID, Cell>,
	games: &Games,
	tasks: &Tasks<Task>,
) -> anyhow::Result<()> {
	for manifest in &games.manifests {
		let (_, button, cell) = construct_game_cover(ess, executor, tasks, globals, manifest)?;

		button.on_click({
			let tasks = tasks.clone();
			let manifest = manifest.clone();
			Box::new(move |_, _| {
				tasks.push(Task::AppManifestClicked(manifest.clone()));
				Ok(())
			})
		});

		cells.insert(manifest.app_id.clone(), cell);
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

	fn refresh(&mut self, layout: &mut Layout, executor: &AsyncExecutor) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);
		self.cells.clear();

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

	fn action_app_manifest_clicked(&mut self, manifest: steam_utils::AppManifest) -> anyhow::Result<()> {
		self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_raw_text(&manifest.name),
			on_content: {
				Rc::new(move |_data| {
					// todo
					Ok(())
				})
			},
		}));

		Ok(())
	}

	fn get_placeholder_image(&mut self) -> anyhow::Result<&CustomGlyphData> {
		if self.img_placeholder.is_none() {
			let c = CustomGlyphData::new(CustomGlyphContent::from_assets(
				&self.globals,
				AssetPath::BuiltIn("dashboard/placeholder_cover.png"),
			)?);
			self.img_placeholder = Some(c);
		}

		Ok(self.img_placeholder.as_ref().unwrap()) // safe
	}

	fn mount_image(layout: &mut Layout, cell: &Cell, glyph: &CustomGlyphData) -> anyhow::Result<()> {
		let image = WidgetImage::create(WidgetImageParams {
			round: WLength::Units(12.0),
			glyph_data: Some(glyph.clone()),
			..Default::default()
		});

		let (a, _) = layout.add_child(
			cell.image_parent,
			image,
			taffy::Style {
				size: taffy::Size {
					width: percent(1.0),
					height: percent(1.0),
				},
				..Default::default()
			},
		)?;
		a.widget.state().flags.new_pass = true;

		Ok(())
	}

	fn mount_placeholder_text(
		globals: &WguiGlobals,
		layout: &mut Layout,
		parent: WidgetID,
		text: &str,
	) -> anyhow::Result<()> {
		let label = WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: Translation::from_raw_text(text),
				style: TextStyle {
					weight: Some(FontWeight::Bold),
					wrap: true,
					size: Some(16.0),
					align: Some(HorizontalAlign::Center),
					shadow: Some(TextShadow {
						color: drawing::Color::new(0.0, 0.0, 0.0, 0.5),
						x: 2.0,
						y: 2.0,
					}),
					..Default::default()
				},
			},
		);

		layout.add_child(
			parent,
			label,
			taffy::Style {
				position: taffy::Position::Absolute,
				align_self: Some(AlignSelf::Baseline),
				justify_self: Some(JustifySelf::Center),
				margin: taffy::Rect {
					top: length(32.0),
					bottom: auto(),
					left: auto(),
					right: auto(),
				},
				..Default::default()
			},
		)?;
		Ok(())
	}

	fn action_set_cover_art(
		&mut self,
		layout: &mut Layout,
		app_id: &AppID,
		cover_art: Rc<CoverArt>,
	) -> anyhow::Result<()> {
		if cover_art.compressed_image_data.is_empty() {
			// mount placeholder
			let img = self.get_placeholder_image()?.clone();

			let Some(cell) = self.cells.get(app_id) else {
				debug_assert!(false); // this shouldn't happen
				return Ok(());
			};

			View::mount_image(layout, cell, &img)?;
			View::mount_placeholder_text(&self.globals, layout, cell.image_parent, &cell.manifest.name)?;
		} else {
			// mount image

			let Some(cell) = self.cells.get(app_id) else {
				debug_assert!(false); // this shouldn't happen
				return Ok(());
			};

			let glyph = match CustomGlyphContent::from_bin_raster(&cover_art.compressed_image_data) {
				Ok(c) => c,
				Err(e) => {
					log::warn!(
						"failed to decode cover art image for AppID {} ({:?}), skipping",
						app_id,
						e
					);
					return Ok(());
				}
			};
			View::mount_image(layout, cell, &CustomGlyphData::new(glyph))?;
		}

		Ok(())
	}
}
