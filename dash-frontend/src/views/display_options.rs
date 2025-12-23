use std::rc::Rc;
use wayvr_ipc::packet_server;
use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::ConstructEssentials,
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	task::Tasks,
	views::display_list::construct_display_button,
};

#[derive(Clone)]
enum Task {
	SetVisible(bool),
	Remove,
}

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	display: packet_server::WvrDisplay,
	on_submit: Rc<dyn Fn()>,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_submit: Rc<dyn Fn()>,
	pub display: packet_server::WvrDisplay,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/display_options.xml"),
			extra: Default::default(),
		};

		let state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let tasks = Tasks::new();

		let display_parent = state.get_widget_id("display_parent")?;
		let btn_remove = state.fetch_component_as::<ComponentButton>("btn_remove")?;
		let btn_show_hide = state.fetch_component_as::<ComponentButton>("btn_show_hide")?;

		construct_display_button(
			&mut ConstructEssentials {
				layout: params.layout,
				parent: display_parent,
			},
			&params.globals,
			&params.display,
		)?;

		{
			let mut c = params.layout.start_common();
			btn_show_hide.set_text(
				&mut c.common(),
				Translation::from_translation_key(if params.display.visible { "HIDE" } else { "SHOW" }),
			);
			c.finish()?;
		}

		tasks.handle_button(btn_remove, Task::Remove);
		tasks.handle_button(btn_show_hide, Task::SetVisible(!params.display.visible));

		Ok(Self {
			state,
			tasks,
			display: params.display,
			frontend_tasks: params.frontend_tasks,
			on_submit: params.on_submit,
		})
	}

	pub fn update(&mut self, _layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::SetVisible(v) => self.action_set_visible(interface, v),
				Task::Remove => self.action_remove(interface),
			}
		}
		Ok(())
	}
}

impl View {
	fn action_set_visible(&mut self, interface: &mut BoxDashInterface, visible: bool) {
		if let Err(e) = interface.display_set_visible(self.display.handle.clone(), visible) {
			self
				.frontend_tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
					"Failed to remove display: {:?}",
					e
				))));
		};

		(*self.on_submit)();
	}

	fn action_remove(&mut self, interface: &mut BoxDashInterface) {
		if let Err(e) = interface.display_remove(self.display.handle.clone()) {
			self
				.frontend_tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
					"Failed to remove display: {:?}",
					e
				))));
		};

		(*self.on_submit)();
	}
}
