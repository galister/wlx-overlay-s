use std::rc::Rc;

use anyhow::Context;
use wgui::{
	assets::AssetPath,
	components::{button::ComponentButton, checkbox::ComponentCheckbox, slider::ComponentSlider},
	event::StyleSetRequest,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	taffy::prelude::length,
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

use crate::{frontend::FrontendTasks, task::Tasks};

#[derive(Clone)]
enum Task {
	Confirm,
	SetWidth(u16),
	SetHeight(u16),
	SetPortrait(bool),
}

pub struct View {
	#[allow(dead_code)]
	pub state: ParserState,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	on_submit: Rc<dyn Fn(Result)>,

	cur_raw_width: u16,
	cur_raw_height: u16,
	cur_display_name: String,
	cur_portrait: bool,

	id_label_width: WidgetID,
	id_label_height: WidgetID,
	id_label_display_name: WidgetID,
	id_rect_display: WidgetID,
	id_label_display: WidgetID,
}

#[derive(Clone)]
pub struct Result {
	pub width: u16,
	pub height: u16,
	pub display_name: String,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub frontend_tasks: FrontendTasks,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_submit: Rc<dyn Fn(Result)>,
}

const RES_COUNT: usize = 7;
const RES_WIDTHS: [u16; RES_COUNT] = [512, 854, 1280, 1600, 1920, 2560, 3840];
const RES_HEIGHTS: [u16; 7] = [256, 480, 720, 900, 1080, 1440, 2160];

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
		let id_label_width = state.get_widget_id("label_width")?;
		let id_label_height = state.get_widget_id("label_height")?;
		let id_label_display_name = state.get_widget_id("label_display_name")?;
		let id_rect_display = state.get_widget_id("rect_display")?;
		let id_label_display = state.get_widget_id("label_display")?;
		let btn_confirm = state.fetch_component_as::<ComponentButton>("btn_confirm")?;
		let cb_portrait = state.fetch_component_as::<ComponentCheckbox>("cb_portrait")?;

		tasks.handle_button(btn_confirm, Task::Confirm);

		// width
		slider_width.on_value_changed({
			let tasks = tasks.clone();
			Box::new(move |_c, e| {
				tasks.push(Task::SetWidth(RES_WIDTHS[e.value as usize]));
				Ok(())
			})
		});

		// height
		slider_height.on_value_changed({
			let tasks = tasks.clone();
			Box::new(move |_c, e| {
				tasks.push(Task::SetHeight(RES_HEIGHTS[e.value as usize]));
				Ok(())
			})
		});

		cb_portrait.on_toggle({
			let tasks = tasks.clone();
			Box::new(move |_c, e| {
				tasks.push(Task::SetPortrait(e.checked));
				Ok(())
			})
		});

		let mut res = Self {
			state,
			tasks,
			frontend_tasks: params.frontend_tasks,
			on_submit: params.on_submit,
			cur_raw_width: RES_WIDTHS[2],
			cur_raw_height: RES_HEIGHTS[2],
			cur_display_name: String::new(),
			cur_portrait: false,
			id_label_width,
			id_label_height,
			id_label_display_name,
			id_rect_display,
			id_label_display,
		};

		res.update_ui(params.layout);

		Ok(res)
	}

	pub fn update(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::Confirm => self.confirm(),
				Task::SetWidth(w) => {
					self.cur_raw_width = w;
					self.update_ui_res(layout)?;
				}
				Task::SetHeight(h) => {
					self.cur_raw_height = h;
					self.update_ui_res(layout)?;
				}
				Task::SetPortrait(p) => {
					self.cur_portrait = p;
					self.update_ui_res(layout)?;
				}
			}
		}
		Ok(())
	}
}

// greatest common divisor
fn gcd(a: u16, b: u16) -> u16 {
	let (mut a, mut b) = (a, b);
	while b != 0 {
		let temp = b;
		b = a % b;
		a = temp;
	}
	a
}

// aspect ratio calculation
// e.g. returns (16, 9) for input values [1280, 720]
fn aspect_ratio(width: u16, height: u16) -> (u16, u16) {
	let gcd = gcd(width, height);
	(width / gcd, height / gcd)
}

impl View {
	fn confirm(&mut self) {
		let (width, height) = self.get_wh();
		(*self.on_submit)(Result {
			width,
			height,
			display_name: self.cur_display_name.clone(),
		});
	}

	fn update_ui(&mut self, layout: &mut Layout) -> Option<()> {
		let mut c = layout.start_common();

		let (cur_width, cur_height) = self.get_wh();

		{
			let mut common = c.common();

			let mut label_width = common.state.widgets.get_as::<WidgetLabel>(self.id_label_width)?;
			let mut label_height = common.state.widgets.get_as::<WidgetLabel>(self.id_label_height)?;
			let mut label_display_name = common.state.widgets.get_as::<WidgetLabel>(self.id_label_display_name)?;
			let mut label_display = common.state.widgets.get_as::<WidgetLabel>(self.id_label_display)?;

			// todo?
			self.cur_display_name = format!("wvr-{}x{}", cur_width, cur_height);

			label_width.set_text(
				&mut common,
				Translation::from_raw_text_string(format!("{}px", self.cur_raw_width)),
			);

			label_height.set_text(
				&mut common,
				Translation::from_raw_text_string(format!("{}px", self.cur_raw_height)),
			);

			let aspect = aspect_ratio(cur_width, cur_height);

			label_display.set_text(
				&mut common,
				Translation::from_raw_text_string(format!("{}x{}\n{}:{}", cur_width, cur_height, aspect.0, aspect.1)),
			);

			label_display_name.set_text(&mut common, Translation::from_raw_text(&self.cur_display_name));

			let mult = 0.1;

			common.alterables.set_style(
				self.id_rect_display,
				StyleSetRequest::Width(length(cur_width as f32 * mult)),
			);

			common.alterables.set_style(
				self.id_rect_display,
				StyleSetRequest::Height(length(cur_height as f32 * mult)),
			);
		}

		c.finish().ok()?;

		Some(())
	}

	fn update_ui_res(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		self.update_ui(layout).context("failed to update ui")?;
		Ok(())
	}

	fn get_wh(&self) -> (u16, u16) {
		if self.cur_portrait {
			(self.cur_raw_height, self.cur_raw_width)
		} else {
			(self.cur_raw_width, self.cur_raw_height)
		}
	}
}
