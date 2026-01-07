use std::{collections::HashMap, rc::Rc};

use glam::Vec2;

use crate::{
	assets::AssetPath,
	components::{button::ComponentButton, ComponentTrait},
	globals::WguiGlobals,
	i18n::Translation,
	layout::Layout,
	parser::{self, Fetchable, ParserState},
	task::Tasks,
	windowing::window::{WguiWindow, WguiWindowParams, WguiWindowParamsExtra},
};

pub struct Cell {
	pub title: Translation,
	pub action_name: Option<Rc<str>>,
	pub attribs: Vec<parser::AttribPair>,
}

pub(crate) struct Blueprint {
	pub cells: Vec<Cell>,
}

pub struct OpenParams {
	pub on_custom_attribs: Option<parser::OnCustomAttribsFunc>,
	pub template_name: Rc<str>,
	pub template_params: HashMap<Rc<str>, Rc<str>>,
	pub position: Vec2,
}

#[derive(Clone)]
enum Task {
	ActionClicked(Option<Rc<str>>),
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
pub enum TickResult {
	/// Nothing happened
	#[default]
	None,
	/// The context menu was opened.
	Opened,
	/// User has selected an action.
	Action(Rc<str>),
	/// The context menu was closed without an action.
	Closed,
}

impl ContextMenu {
	pub fn open(&mut self, params: OpenParams) {
		self.pending_open = Some(params);
	}

	pub fn close(&self) {
		self.window.close();
	}

	fn open_process(
		&mut self,
		params: &mut OpenParams,
		layout: &mut Layout,
		parser_state: &mut ParserState,
	) -> anyhow::Result<()> {
		let blueprint = parser_state.context_menu_create_blueprint(&params.template_name, &params.template_params)?;

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
		let doc_params = doc_params(&globals);

		let mut inner_parser = parser::parse_from_assets(&doc_params, layout, content.id)?;

		let id_buttons = inner_parser.get_widget_id("buttons")?;

		for (idx, cell) in blueprint.cells.iter().enumerate() {
			let mut par = HashMap::new();
			par.insert(Rc::from("text"), cell.title.generate(&mut globals.i18n()));
			let mut data_cell = inner_parser.parse_template(&doc_params, "Cell", layout, id_buttons, par)?;

			let button = data_cell.fetch_component_as::<ComponentButton>("button")?;
			let button_id = button.base().get_id();
			parser_state.data.take_results_from(&mut data_cell);
			self
				.tasks
				.handle_button(&button, Task::ActionClicked(cell.action_name.clone()));

			if let Some(c) = &mut params.on_custom_attribs {
				(*c)(parser::CustomAttribsInfo {
					pairs: &cell.attribs,
					parent_id: id_buttons,
					widget_id: button_id,
					widgets: &layout.state.widgets,
				});
			}

			if idx < blueprint.cells.len() - 1 {
				inner_parser.parse_template(&doc_params, "Separator", layout, id_buttons, Default::default())?;
			}
		}
		Ok(())
	}

	pub fn tick(&mut self, layout: &mut Layout, parser_state: &mut ParserState) -> anyhow::Result<TickResult> {
		if let Some(mut p) = self.pending_open.take() {
			self.open_process(&mut p, layout, parser_state)?;
			let _ = self.tasks.drain();
			return Ok(TickResult::Opened);
		}

		let mut result = TickResult::default();

		for task in self.tasks.drain() {
			match task {
				Task::ActionClicked(Some(action_name)) => {
					result = TickResult::Action(action_name);
					self.close();
				}
				Task::ActionClicked(None) => {
					result = TickResult::Closed;
					self.close();
				}
			}
		}

		Ok(result)
	}
}
