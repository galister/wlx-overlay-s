use anyhow::Context;
use std::rc::Rc;
use wayvr_ipc::packet_server;
use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
	widget::ConstructEssentials,
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	views::window_list::construct_window_button,
};

#[derive(Clone)]
enum Task {
	SetVisible(bool),
	Kill,
	Close,
}

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	window: packet_server::WvrWindow,
	on_submit: Rc<dyn Fn()>,
}

pub struct Params<'a, T> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_submit: Rc<dyn Fn()>,
	pub window: packet_server::WvrWindow,
	pub interface: &'a mut BoxDashInterface<T>,
	pub data: &'a mut T,
}

impl View {
	pub fn new<T>(params: Params<T>) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/window_options.xml"),
			extra: Default::default(),
		};

		let state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let tasks = Tasks::new();

		let window_parent = state.get_widget_id("window_parent")?;
		let btn_close = state.fetch_component_as::<ComponentButton>("btn_close")?;
		let btn_kill = state.fetch_component_as::<ComponentButton>("btn_kill")?;
		let btn_show_hide = state.fetch_component_as::<ComponentButton>("btn_show_hide")?;

		construct_window_button(
			&mut ConstructEssentials {
				layout: params.layout,
				parent: window_parent,
			},
			params.interface,
			params.data,
			&params.globals,
			&params.window,
		)?;

		{
			let mut c = params.layout.start_common();
			btn_show_hide.set_text(
				&mut c.common(),
				Translation::from_translation_key(if params.window.visible { "HIDE" } else { "SHOW" }),
			);
			c.finish()?;
		}

		tasks.handle_button(&btn_close, Task::Close);
		tasks.handle_button(&btn_kill, Task::Kill);
		tasks.handle_button(&btn_show_hide, Task::SetVisible(!params.window.visible));

		Ok(Self {
			state,
			tasks,
			window: params.window,
			frontend_tasks: params.frontend_tasks,
			on_submit: params.on_submit,
		})
	}

	pub fn update<T>(
		&mut self,
		_layout: &mut Layout,
		interface: &mut BoxDashInterface<T>,
		data: &mut T,
	) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::SetVisible(v) => self.action_set_visible(interface, data, v),
				Task::Close => self.action_close(interface, data),
				Task::Kill => self.action_kill(interface, data),
			}
		}
		Ok(())
	}
}

impl View {
	fn action_set_visible<T>(&mut self, interface: &mut BoxDashInterface<T>, data: &mut T, visible: bool) {
		if let Err(e) = interface.window_set_visible(data, self.window.handle.clone(), visible) {
			self
				.frontend_tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
					"Failed to set window visibility: {:?}",
					e
				))));
		};

		(*self.on_submit)();
	}

	fn action_close<T>(&mut self, interface: &mut BoxDashInterface<T>, data: &mut T) {
		if let Err(e) = interface.window_request_close(data, self.window.handle.clone()) {
			self
				.frontend_tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
					"Failed to close window: {:?}",
					e
				))));
		};

		(*self.on_submit)();
	}

	fn action_kill_process<T>(&mut self, interface: &mut BoxDashInterface<T>, data: &mut T) -> anyhow::Result<()> {
		let process = interface
			.process_get(data, self.window.process_handle.clone())
			.context("Process not found")?;
		interface.process_terminate(data, process.handle)?;
		Ok(())
	}

	fn action_kill<T>(&mut self, interface: &mut BoxDashInterface<T>, data: &mut T) {
		if let Err(e) = self.action_kill_process(interface, data) {
			self
				.frontend_tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(format!(
					"Failed to kill process: {:?}",
					e
				))));
		};

		(*self.on_submit)();
	}
}
