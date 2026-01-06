use std::{collections::HashMap, rc::Rc};

use glam::Vec2;

use crate::{
	assets::AssetPath,
	components::button::ComponentButton,
	event::CallbackDataCommon,
	globals::WguiGlobals,
	i18n::Translation,
	layout::Layout,
	parser::{self, Fetchable},
	task::Tasks,
	windowing::window::{WguiWindow, WguiWindowParams, WguiWindowParamsExtra},
};

pub struct Cell {
	pub title: Translation,
	pub action_name: Rc<str>,
}

pub struct Blueprint {
	pub cells: Vec<Cell>,
}

pub struct ContextMenuAction<'a> {
	pub common: &'a mut CallbackDataCommon<'a>,
	pub name: Rc<str>, // action name
}

pub struct OpenParams {
	pub position: Vec2,
	pub data: Blueprint,
}

#[derive(Clone)]
enum Task {
	ActionClicked(Rc<str>),
}

#[derive(Default)]
pub struct ContextMenu {
	window: WguiWindow,
	pending_open: Option<OpenParams>,
	tasks: Tasks<Task>,
}

fn doc_params<'a>(globals: &WguiGlobals) -> parser::ParseDocumentParams<'a> {
	parser::ParseDocumentParams {
		globals: globals.clone(),
		path: AssetPath::WguiInternal("wgui/context_menu.xml"),
		extra: Default::default(),
	}
}

#[derive(Default)]
pub struct TickResult {
	pub action_name: Option<Rc<str>>,
}

impl ContextMenu {
	pub fn open(&mut self, params: OpenParams) {
		self.pending_open = Some(params);
	}

	pub fn close(&self) {
		self.window.close();
	}

	fn open_process(&mut self, params: &OpenParams, layout: &mut Layout) -> anyhow::Result<()> {
		let globals = layout.state.globals.clone();

		self.window.open(&mut WguiWindowParams {
			globals: &globals,
			layout,
			position: params.position,
			extra: WguiWindowParamsExtra {
				with_decorations: false,
				close_if_clicked_outside: true,
				..Default::default()
			},
		})?;

		let content = self.window.get_content();

		let mut state = parser::parse_from_assets(&doc_params(&globals), layout, content.id)?;

		let id_buttons = state.get_widget_id("buttons")?;

		for (idx, cell) in params.data.cells.iter().enumerate() {
			let mut par = HashMap::new();
			par.insert(Rc::from("text"), cell.title.generate(&mut globals.i18n()));
			let data_cell = state.parse_template(&doc_params(&globals), "Cell", layout, id_buttons, par)?;

			let button = data_cell.fetch_component_as::<ComponentButton>("button")?;
			self
				.tasks
				.handle_button(&button, Task::ActionClicked(cell.action_name.clone()));

			if idx < params.data.cells.len() - 1 {
				state.parse_template(
					&doc_params(&globals),
					"Separator",
					layout,
					id_buttons,
					Default::default(),
				)?;
			}
		}

		Ok(())
	}

	pub fn tick(&mut self, layout: &mut Layout) -> anyhow::Result<TickResult> {
		if let Some(p) = self.pending_open.take() {
			self.open_process(&p, layout)?;
		}

		let mut result = TickResult::default();

		for task in self.tasks.drain() {
			match task {
				Task::ActionClicked(action_name) => {
					result.action_name = Some(action_name);
					self.close();
				}
			}
		}

		Ok(result)
	}
}
