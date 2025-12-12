use std::{cell::RefCell, rc::Rc};

use glam::Vec2;
use taffy::prelude::{length, percent};

use crate::{
	assets::AssetPath,
	components::button::ComponentButton,
	event::StyleSetRequest,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, LayoutTask, LayoutTasks, WidgetPair},
	parser::{self, Fetchable, ParserState},
	widget::{div::WidgetDiv, label::WidgetLabel},
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

#[derive(Default)]
pub enum WguiWindowPlacement {
	#[default]
	TopLeft,
	BottomLeft,
	TopRight,
	BottomRight,
}

#[derive(Default)]
pub struct WguiWindowParamsExtra {
	pub fixed_width: Option<f32>,
	pub fixed_height: Option<f32>,
	pub placement: WguiWindowPlacement,
}

pub struct WguiWindowParams<'a> {
	pub position: Vec2,
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub title: Translation,
	pub extra: WguiWindowParamsExtra,
}

impl Default for WguiWindow {
	fn default() -> Self {
		Self(Rc::new(RefCell::new(State { opened_window: None })))
	}
}

const WINDOW_DECORATION_HEADER_HEIGHT: f32 = 32.0;
const WINDOW_DECORATION_PADDING: f32 = 2.0;

impl WguiWindow {
	pub fn close(&self) {
		self.0.borrow_mut().opened_window = None;
	}

	pub fn open(&mut self, params: &mut WguiWindowParams) -> anyhow::Result<()> {
		// close previous one if it's already open
		self.close();

		const XML_PATH: AssetPath = AssetPath::WguiInternal("wgui/window_frame.xml");

		let (padding, justify_content, align_items) = match params.extra.placement {
			WguiWindowPlacement::TopLeft => (
				taffy::Rect {
					left: length(params.position.x - WINDOW_DECORATION_PADDING),
					top: length(params.position.y - WINDOW_DECORATION_HEADER_HEIGHT - WINDOW_DECORATION_PADDING),
					bottom: length(0.0),
					right: length(0.0),
				},
				taffy::JustifyContent::Start, // x start
				taffy::AlignItems::Start,     // y start
			),
			WguiWindowPlacement::BottomLeft => (
				taffy::Rect {
					left: length(params.position.x - WINDOW_DECORATION_PADDING),
					top: length(0.0),
					bottom: length(params.position.y - WINDOW_DECORATION_PADDING),
					right: length(0.0),
				},
				taffy::JustifyContent::Start, // x start
				taffy::AlignItems::End,       // y end
			),
			WguiWindowPlacement::TopRight => (
				taffy::Rect {
					left: length(0.0),
					top: length(params.position.y - WINDOW_DECORATION_HEADER_HEIGHT - WINDOW_DECORATION_PADDING),
					bottom: length(0.0),
					right: length(params.position.x - WINDOW_DECORATION_PADDING),
				},
				taffy::JustifyContent::End, // x end
				taffy::AlignItems::Start,   // y start
			),
			WguiWindowPlacement::BottomRight => (
				taffy::Rect {
					left: length(0.0),
					top: length(0.0),
					bottom: length(params.position.y - WINDOW_DECORATION_PADDING),
					right: length(params.position.x - WINDOW_DECORATION_PADDING),
				},
				taffy::JustifyContent::End, // x end
				taffy::AlignItems::End,     // y end
			),
		};

		let (widget, _) = params.layout.add_topmost_child(
			WidgetDiv::create(),
			taffy::Style {
				position: taffy::Position::Absolute,
				align_items: Some(align_items),
				justify_content: Some(justify_content),
				padding,
				size: taffy::Size {
					width: percent(1.0),
					height: percent(1.0),
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

		{
			let mut text_title = state.fetch_widget_as::<WidgetLabel>(&params.layout.state, "text_window_title")?;
			text_title.set_text_simple(&mut params.globals.get(), params.title.clone());
		}

		let content = state.fetch_widget(&params.layout.state, "content")?;

		self.0.borrow_mut().opened_window = Some(OpenedWindow {
			widget,
			state,
			layout_tasks: params.layout.tasks.clone(),
			content: content.clone(),
		});

		let mut c = params.layout.start_common();
		if let Some(width) = params.extra.fixed_width {
			c.common()
				.alterables
				.set_style(content.id, StyleSetRequest::Width(length(width)));
		}

		if let Some(height) = params.extra.fixed_height {
			c.common()
				.alterables
				.set_style(content.id, StyleSetRequest::Height(length(height)));
		}

		c.finish()?;

		Ok(())
	}

	pub fn get_content(&self) -> WidgetPair {
		let state = self.0.borrow_mut();
		state.opened_window.as_ref().unwrap().content.clone()
	}
}
