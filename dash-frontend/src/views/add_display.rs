use std::{collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::{button::ComponentButton, slider::ComponentSlider},
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

use crate::{frontend::FrontendTasks, tab::TabUpdateParams, task::Tasks};

#[derive(Clone)]
enum Task {
	Confirm,
}

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	on_submit: Rc<dyn Fn()>,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_submit: Rc<dyn Fn()>,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/add_display.xml"),
			extra: Default::default(),
		};

		let state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let tasks = Tasks::new();

		let slider_width = state.fetch_component_as::<ComponentSlider>("slider_width")?;
		let slider_height = state.fetch_component_as::<ComponentSlider>("slider_height")?;
		let label_display_name = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_display_name")?;
		let rect_display = state.fetch_widget_as::<WidgetRectangle>(&params.layout.state, "rect_display");
		let label_display = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "label_display");
		let btn_confirm = state.fetch_component_as::<ComponentButton>("btn_confirm")?;

		tasks.handle_button(btn_confirm, Task::Confirm);

		Ok(Self {
			state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			on_submit: params.on_submit,
		})
	}

	pub fn update(&mut self) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::Confirm => self.confirm(),
			}
		}
		Ok(())
	}
}

impl View {
	fn confirm(&mut self) {
		log::info!("confirm");
		(*self.on_submit)();
	}
}
