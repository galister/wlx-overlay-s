use std::{cell::RefCell, rc::Rc};

use wayvr_ipc::packet_server::{self, WvrWindowHandle};
use wgui::{
	assets::AssetPath,
	components::{self, button::ComponentButton},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID, WidgetPair},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	renderer_vk::text::{FontWeight, HorizontalAlign, TextStyle},
	taffy::{self, prelude::length},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		ConstructEssentials,
	},
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	task::Tasks,
	util::popup_manager::{MountPopupParams, PopupHandle},
	views::window_options,
};

#[derive(Clone)]
enum Task {
	WindowClicked(packet_server::WvrWindow),
	WindowOptionsFinish,
	Refresh,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_click: Option<Box<dyn Fn(WvrWindowHandle)>>,
}

struct State {
	view_window_options: Option<(PopupHandle, window_options::View)>,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	state: Rc<RefCell<State>>,
	id_list_parent: WidgetID,
	on_click: Option<Box<dyn Fn(WvrWindowHandle)>>,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/window_list.xml"),
			extra: Default::default(),
		};

		let parser_state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;
		let list_parent = parser_state.fetch_widget(&params.layout.state, "list_parent")?;

		let tasks = Tasks::new();

		tasks.push(Task::Refresh);

		let state = Rc::new(RefCell::new(State {
			view_window_options: None,
		}));

		Ok(Self {
			parser_state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			globals: params.globals.clone(),
			state,
			id_list_parent: list_parent.id,
			on_click: params.on_click,
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
					Task::WindowClicked(display) => self.action_window_clicked(display)?,
					Task::WindowOptionsFinish => self.action_window_options_finish(),
					Task::Refresh => self.refresh(layout, interface)?,
				}
			}
		}

		let mut state = self.state.borrow_mut();
		if let Some((_, view)) = &mut state.view_window_options {
			view.update(layout, interface)?;
		}

		Ok(())
	}
}

pub fn construct_window_button(
	ess: &mut ConstructEssentials,
	interface: &mut BoxDashInterface,
	globals: &WguiGlobals,
	window: &packet_server::WvrWindow,
) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>)> {
	let aspect = window.size_x as f32 / window.size_y as f32;
	let height = 96.0;
	let width = height * aspect;
	let accent_color = globals.defaults().accent_color;

	let (widget_button, button) = components::button::construct(
		ess,
		components::button::Params {
			color: Some(accent_color.with_alpha(0.2)),
			border_color: Some(accent_color),
			style: taffy::Style {
				align_items: Some(taffy::AlignItems::Center),
				justify_content: Some(taffy::JustifyContent::Center),
				size: taffy::Size {
					width: length(width),
					height: length(height),
				},
				..Default::default()
			},
			..Default::default()
		},
	)?;

	let process_name = match interface.process_get(window.process_handle.clone()) {
		Some(process) => process.name.clone(),
		None => String::from("Unknown"),
	};

	let label_name = WidgetLabel::create(
		&mut globals.get(),
		WidgetLabelParams {
			content: Translation::from_raw_text(&process_name),
			style: TextStyle {
				weight: Some(FontWeight::Bold),
				wrap: true,
				align: Some(HorizontalAlign::Center),
				..Default::default()
			},
		},
	);

	let label_resolution = WidgetLabel::create(
		&mut globals.get(),
		WidgetLabelParams {
			content: Translation::from_raw_text(""),
			..Default::default()
		},
	);

	ess.layout.add_child(widget_button.id, label_name, Default::default())?;
	ess
		.layout
		.add_child(widget_button.id, label_resolution, Default::default())?;

	Ok((widget_button, button))
}

fn fill_window_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	interface: &mut BoxDashInterface,
	list: Vec<packet_server::WvrWindow>,
	tasks: &Tasks<Task>,
) -> anyhow::Result<()> {
	for entry in list {
		let (_, button) = construct_window_button(ess, interface, globals, &entry)?;

		button.on_click({
			let tasks = tasks.clone();
			Box::new(move |_, _| {
				tasks.push(Task::WindowClicked(entry.clone()));
				Ok(())
			})
		});
	}

	Ok(())
}

impl View {
	fn action_window_options_finish(&mut self) {
		self.state.borrow_mut().view_window_options = None;
		self.tasks.push(Task::Refresh);
	}

	fn refresh(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);

		let mut text: Option<Translation> = None;
		match interface.window_list() {
			Ok(list) => {
				if list.is_empty() {
					text = Some(Translation::from_translation_key("NO_WINDOWS_FOUND"))
				} else {
					fill_window_list(
						&self.globals,
						&mut ConstructEssentials {
							layout,
							parent: self.id_list_parent,
						},
						interface,
						list,
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

	fn action_window_clicked(&mut self, window: packet_server::WvrWindow) -> anyhow::Result<()> {
		if let Some(on_click) = &mut self.on_click {
			(*on_click)(window.handle);
		} else {
			self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
				title: Translation::from_translation_key("WINDOW_OPTIONS"),
				on_content: {
					let frontend_tasks = self.frontend_tasks.clone();
					let globals = self.globals.clone();
					let state = self.state.clone();
					let tasks = self.tasks.clone();

					Rc::new(move |data| {
						state.borrow_mut().view_window_options = Some((
							data.handle,
							window_options::View::new(window_options::Params {
								globals: globals.clone(),
								layout: data.layout,
								parent_id: data.id_content,
								on_submit: tasks.make_callback(Task::WindowOptionsFinish),
								window: window.clone(),
								frontend_tasks: frontend_tasks.clone(),
								interface: data.interface,
							})?,
						));
						Ok(())
					})
				},
			}));
		}

		Ok(())
	}
}
