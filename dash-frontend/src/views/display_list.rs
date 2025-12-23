use std::{cell::RefCell, rc::Rc};

use wayvr_ipc::{
	packet_client::{self},
	packet_server::{self, WvrDisplayHandle},
};
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
		ConstructEssentials,
		label::{WidgetLabel, WidgetLabelParams},
	},
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	task::Tasks,
	util::popup_manager::{MountPopupParams, PopupHandle},
	views::{add_display, display_options},
};

#[derive(Clone)]
enum Task {
	AddDisplay,
	AddDisplayFinish(add_display::Result),
	DisplayClicked(packet_server::WvrDisplay),
	DisplayOptionsFinish,
	Refresh,
}

pub struct Params<'a> {
	pub globals: &'a WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_click: Option<Box<dyn Fn(WvrDisplayHandle)>>,
}

struct State {
	view_add_display: Option<(PopupHandle, add_display::View)>,
	view_display_options: Option<(PopupHandle, display_options::View)>,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	state: Rc<RefCell<State>>,
	id_list_parent: WidgetID,
	on_click: Option<Box<dyn Fn(WvrDisplayHandle)>>,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/display_list.xml"),
			extra: Default::default(),
		};

		let parser_state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;
		let list_parent = parser_state.fetch_widget(&params.layout.state, "list_parent")?;

		let tasks = Tasks::new();

		let btn_add = parser_state.fetch_component_as::<ComponentButton>("btn_add")?;
		tasks.handle_button(btn_add, Task::AddDisplay);
		tasks.push(Task::Refresh);

		let state = Rc::new(RefCell::new(State {
			view_add_display: None,
			view_display_options: None,
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
					Task::AddDisplay => self.action_add_display(),
					Task::AddDisplayFinish(result) => self.action_add_display_finish(interface, result)?,
					Task::DisplayOptionsFinish => self.action_display_options_finish(),
					Task::Refresh => self.refresh(layout, interface)?,
					Task::DisplayClicked(display) => self.action_display_clicked(display)?,
				}
			}
		}

		let mut state = self.state.borrow_mut();
		if let Some((_, view)) = &mut state.view_add_display {
			view.update(layout)?;
		}

		if let Some((_, view)) = &mut state.view_display_options {
			view.update(layout, interface)?;
		}

		Ok(())
	}
}

pub fn construct_display_button(
	ess: &mut ConstructEssentials,
	globals: &WguiGlobals,
	display: &packet_server::WvrDisplay,
) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>)> {
	let aspect = display.width as f32 / display.height as f32;
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

	let label_name = WidgetLabel::create(
		&mut globals.get(),
		WidgetLabelParams {
			content: Translation::from_raw_text(&display.name),
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

fn fill_display_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	list: Vec<packet_server::WvrDisplay>,
	tasks: &Tasks<Task>,
) -> anyhow::Result<()> {
	for entry in list {
		let (_, button) = construct_display_button(ess, globals, &entry)?;

		button.on_click({
			let tasks = tasks.clone();
			Box::new(move |_, _| {
				tasks.push(Task::DisplayClicked(entry.clone()));
				Ok(())
			})
		});
	}

	Ok(())
}

impl View {
	fn action_add_display(&mut self) {
		self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_translation_key("ADD_DISPLAY"),
			on_content: {
				let globals = self.globals.clone();
				let state = self.state.clone();

				let on_submit = {
					let tasks = self.tasks.clone();
					Rc::new(move |result| {
						tasks.push(Task::AddDisplayFinish(result));
						tasks.push(Task::Refresh);
					})
				};

				Rc::new(move |data| {
					state.borrow_mut().view_add_display = Some((
						data.handle,
						add_display::View::new(add_display::Params {
							globals: globals.clone(),
							layout: data.layout,
							parent_id: data.id_content,
							on_submit: on_submit.clone(),
						})?,
					));
					Ok(())
				})
			},
		}));
	}

	fn action_add_display_finish(
		&mut self,
		interface: &mut BoxDashInterface,
		result: add_display::Result,
	) -> anyhow::Result<()> {
		interface.display_create(packet_client::WvrDisplayCreateParams {
			width: result.width,
			height: result.height,
			name: result.display_name,
			attach_to: packet_client::AttachTo::None,
			scale: None,
		})?;
		self.state.borrow_mut().view_add_display = None;
		Ok(())
	}

	fn action_display_options_finish(&mut self) {
		self.state.borrow_mut().view_display_options = None;
		self.tasks.push(Task::Refresh);
	}

	fn refresh(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);

		let mut text: Option<Translation> = None;
		match interface.display_list() {
			Ok(list) => {
				if list.is_empty() {
					text = Some(Translation::from_translation_key("NO_DISPLAYS_FOUND"))
				} else {
					fill_display_list(
						&self.globals,
						&mut ConstructEssentials {
							layout,
							parent: self.id_list_parent,
						},
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

	fn action_display_clicked(&mut self, display: packet_server::WvrDisplay) -> anyhow::Result<()> {
		if let Some(on_click) = &mut self.on_click {
			(*on_click)(display.handle);
		} else {
			self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
				title: Translation::from_translation_key("DISPLAY_OPTIONS"),
				on_content: {
					let frontend_tasks = self.frontend_tasks.clone();
					let globals = self.globals.clone();
					let state = self.state.clone();
					let tasks = self.tasks.clone();

					Rc::new(move |data| {
						state.borrow_mut().view_display_options = Some((
							data.handle,
							display_options::View::new(display_options::Params {
								globals: globals.clone(),
								layout: data.layout,
								parent_id: data.id_content,
								on_submit: tasks.make_callback(Task::DisplayOptionsFinish),
								display: display.clone(),
								frontend_tasks: frontend_tasks.clone(),
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
