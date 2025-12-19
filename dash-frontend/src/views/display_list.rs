use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wayvr_ipc::{
	packet_client::{AttachTo, WvrDisplayCreateParams},
	packet_server::WvrDisplay,
};
use wgui::{
	assets::AssetPath,
	components::{self, button::ComponentButton},
	drawing::Color,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	renderer_vk::text::{FontWeight, HorizontalAlign, TextStyle},
	taffy::{self, prelude::length},
	widget::{
		ConstructEssentials,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};
use wlx_common::dash_interface;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	tab::TabUpdateParams,
	task::Tasks,
	util::popup_manager::{MountPopupParams, PopupHandle},
	views::add_display,
};

#[derive(Clone)]
enum Task {
	AddDisplay,
	AddDisplayFinish(add_display::Result),
	Refresh,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

struct State {
	view_add_display: Option<(PopupHandle, add_display::View)>,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	state: Rc<RefCell<State>>,
	id_list_parent: WidgetID,
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

		let state = Rc::new(RefCell::new(State { view_add_display: None }));

		Ok(Self {
			parser_state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			globals: params.globals,
			state,
			id_list_parent: list_parent.id,
		})
	}

	pub fn update(
		&mut self,
		layout: &mut Layout,
		interface: &mut Box<dyn dash_interface::DashInterface>,
	) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::AddDisplay => self.add_display(),
					Task::AddDisplayFinish(result) => self.add_display_finish(interface, result)?,
					Task::Refresh => self.refresh(layout, interface)?,
				}
			}
		}

		let mut state = self.state.borrow_mut();
		if let Some((_, view)) = &mut state.view_add_display {
			view.update(layout)?;
		}

		Ok(())
	}
}

fn fill_display_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	list: Vec<WvrDisplay>,
) -> anyhow::Result<()> {
	let accent_color = globals.defaults().accent_color;

	for entry in list {
		let aspect = entry.width as f32 / entry.height as f32;

		let height = 96.0;
		let width = height * aspect;

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

		button.on_click(Box::new(move |_, _| {
			log::error!("display options todo");
			Ok(())
		}));

		let label_name = WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: Translation::from_raw_text(&entry.name),
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
	}

	Ok(())
}

impl View {
	fn add_display(&mut self) {
		self.frontend_tasks.push(FrontendTask::MountPopup(MountPopupParams {
			title: Translation::from_translation_key("ADD_DISPLAY"),
			on_content: {
				let frontend_tasks = self.frontend_tasks.clone();
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
							frontend_tasks: frontend_tasks.clone(),
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

	fn add_display_finish(
		&mut self,
		interface: &mut Box<dyn dash_interface::DashInterface>,
		result: add_display::Result,
	) -> anyhow::Result<()> {
		interface.display_create(WvrDisplayCreateParams {
			width: result.width,
			height: result.height,
			name: result.display_name,
			attach_to: AttachTo::None,
			scale: None,
		})?;
		self.state.borrow_mut().view_add_display = None;
		Ok(())
	}

	fn refresh(
		&mut self,
		layout: &mut Layout,
		interface: &mut Box<dyn dash_interface::DashInterface>,
	) -> anyhow::Result<()> {
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
}
