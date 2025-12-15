use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wayvr_ipc::packet_server::WvrDisplay;
use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	renderer_vk::text::{FontWeight, TextStyle},
	taffy,
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
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
	AddDisplayFinish,
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
					Task::AddDisplayFinish => self.add_display_finish()?,
					Task::Refresh => self.refresh(layout, interface)?,
				}
			}
		}

		let mut state = self.state.borrow_mut();
		if let Some((_, view)) = &mut state.view_add_display {
			view.update()?;
		}

		Ok(())
	}
}

fn fill_display_list(
	globals: &WguiGlobals,
	parent: WidgetID,
	layout: &mut Layout,
	list: Vec<WvrDisplay>,
) -> anyhow::Result<()> {
	for entry in list {
		let (rect, _) = layout.add_child(
			parent,
			WidgetRectangle::create(WidgetRectangleParams { ..Default::default() }),
			taffy::Style {
				align_items: Some(taffy::AlignItems::Center),
				justify_content: Some(taffy::JustifyContent::Center),
				..Default::default()
			},
		)?;

		let label_name = WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: Translation::from_raw_text(&entry.name),
				style: TextStyle {
					weight: Some(FontWeight::Bold),
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

		layout.add_child(rect.id, label_name, Default::default())?;
		layout.add_child(rect.id, label_resolution, Default::default())?;
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
				let tasks = self.tasks.clone();

				Rc::new(move |data| {
					state.borrow_mut().view_add_display = Some((
						data.handle,
						add_display::View::new(add_display::Params {
							frontend_tasks: frontend_tasks.clone(),
							globals: globals.clone(),
							layout: data.layout,
							parent_id: data.id_content,
							on_submit: tasks.make_callback(Task::AddDisplayFinish),
						})?,
					));
					Ok(())
				})
			},
		}));
	}

	fn add_display_finish(&mut self) -> anyhow::Result<()> {
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
					fill_display_list(&self.globals, self.id_list_parent, layout, list)?
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
