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
	windowing::window::{WguiWindow, WguiWindowParams, WguiWindowParamsExtra},
};

pub struct Cell {
	pub title: Translation,
	pub action_name: Rc<str>,
}

pub struct ContextMenuAction<'a> {
	pub common: &'a mut CallbackDataCommon<'a>,
	pub name: Rc<str>, // action name
}

pub struct OpenParams<'a> {
	pub position: Vec2,
	pub globals: &'a WguiGlobals,
	pub layout: &'a mut Layout,
	pub on_action: Rc<dyn Fn(ContextMenuAction)>,
	pub cells: Vec<Cell>,
}

#[derive(Default)]
pub struct ContextMenu {
	window: WguiWindow,
}

fn doc_params<'a>(globals: WguiGlobals) -> parser::ParseDocumentParams<'a> {
	parser::ParseDocumentParams {
		globals,
		path: AssetPath::WguiInternal("wgui/context_menu.xml"),
		extra: Default::default(),
	}
}

impl ContextMenu {
	pub fn open(&mut self, params: &mut OpenParams) -> anyhow::Result<()> {
		self.window.open(&mut WguiWindowParams {
			globals: params.globals,
			layout: params.layout,
			position: params.position,
			extra: WguiWindowParamsExtra {
				with_decorations: false,
				close_if_clicked_outside: true,
				..Default::default()
			},
		})?;

		let content = self.window.get_content();

		let mut state = parser::parse_from_assets(&doc_params(params.globals.clone()), params.layout, content.id)?;

		let id_buttons = state.get_widget_id("buttons")?;

		for (idx, cell) in params.cells.iter().enumerate() {
			let mut par = HashMap::new();
			par.insert(Rc::from("text"), cell.title.generate(&mut params.globals.i18n()));
			let data_cell = state.parse_template(
				&doc_params(params.globals.clone()),
				"Cell",
				params.layout,
				id_buttons,
				par,
			)?;

			let button = data_cell.fetch_component_as::<ComponentButton>("button")?;
			button.on_click({
				let on_action = params.on_action.clone();
				let name = cell.action_name.clone();
				let window = self.window.clone();
				Box::new(move |common, _| {
					(*on_action)(ContextMenuAction {
						name: name.clone(),
						// FIXME: why i can't just provide this as-is!?
						/* common: common, */
						common: &mut CallbackDataCommon {
							alterables: common.alterables,
							state: common.state,
						},
					});
					window.close();
					Ok(())
				})
			});

			if idx < params.cells.len() - 1 {
				state.parse_template(
					&doc_params(params.globals.clone()),
					"Separator",
					params.layout,
					id_buttons,
					Default::default(),
				)?;
			}
		}

		Ok(())
	}

	pub fn close(&self) {
		self.window.close();
	}
}
