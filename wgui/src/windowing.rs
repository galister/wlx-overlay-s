use std::{cell::RefCell, rc::Rc};

use glam::Vec2;
use taffy::prelude::length;

use crate::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	layout::{Layout, LayoutTask, LayoutTasks, WidgetPair},
	parser::{self, Fetchable, ParserState},
	widget::div::WidgetDiv,
};

struct OpenedWindow {
	layout_tasks: LayoutTasks,
	widget: WidgetPair,
	content: WidgetPair,

	#[allow(dead_code)]
	state: ParserState,
}

impl Drop for OpenedWindow {
	fn drop(&mut self) {
		self.layout_tasks.push(LayoutTask::RemoveWidget(self.widget.id));
	}
}

struct State {
	opened_window: Option<OpenedWindow>,
}

#[derive(Clone)]
pub struct WguiWindow(Rc<RefCell<State>>);

pub struct OnContentData {
	pub widget: WidgetPair,
}

pub struct WguiWindowParams<'a> {
	pub position: Vec2,
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
}

impl Default for WguiWindow {
	fn default() -> Self {
		Self(Rc::new(RefCell::new(State { opened_window: None })))
	}
}

impl WguiWindow {
	pub fn close(&self) {
		self.0.borrow_mut().opened_window = None;
	}

	pub fn open(&mut self, params: &mut WguiWindowParams) -> anyhow::Result<()> {
		// close previous one if it's already open
		self.close();

		const XML_PATH: AssetPath = AssetPath::WguiInternal("wgui/window_frame.xml");

		let (widget, _) = params.layout.add_topmost_child(
			WidgetDiv::create(),
			taffy::Style {
				position: taffy::Position::Absolute,
				margin: taffy::Rect {
					left: length(params.position.x),
					right: length(0.0),
					top: length(params.position.y),
					bottom: length(0.0),
				},
				..Default::default()
			},
		)?;

		let state = parser::parse_from_assets(
			&parser::ParseDocumentParams {
				globals: params.globals.clone(),
				path: XML_PATH,
				extra: Default::default(),
			},
			params.layout,
			widget.id,
		)?;

		let but_close = state.fetch_component_as::<ComponentButton>("but_close").unwrap();
		but_close.on_click({
			let this = self.clone();
			Box::new(move |_common, _e| {
				this.close();
				Ok(())
			})
		});

		let content = state.fetch_widget(&params.layout.state, "content")?;

		self.0.borrow_mut().opened_window = Some(OpenedWindow {
			widget,
			state,
			layout_tasks: params.layout.tasks.clone(),
			content,
		});

		Ok(())
	}

	pub fn get_content(&self) -> WidgetPair {
		let state = self.0.borrow_mut();
		state.opened_window.as_ref().unwrap().content.clone()
	}
}
