use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{
	assets,
	testbed::{Testbed, TestbedUpdateParams},
};
use glam::Vec2;
use wgui::{
	assets::AssetPath,
	components::{
		Component,
		button::{ButtonClickCallback, ComponentButton},
		checkbox::ComponentCheckbox,
	},
	drawing::Color,
	event::EventListenerCollection,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, LayoutParams, RcLayout, Widget},
	parser::{Fetchable, ParseDocumentExtra, ParseDocumentParams, ParserState},
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
	windowing::{WguiWindow, WguiWindowParams},
};

pub enum TestbedTask {
	ShowPopup,
}

struct Data {
	tasks: VecDeque<TestbedTask>,
	#[allow(dead_code)]
	state: ParserState,

	popup_window: WguiWindow,
}

#[derive(Clone)]
pub struct TestbedGeneric {
	pub layout: RcLayout,

	globals: WguiGlobals,
	data: Rc<RefCell<Data>>,
}

fn button_click_callback(
	button: Component,
	label: Widget,
	text: &'static str,
) -> ButtonClickCallback {
	Box::new(move |common, _e| {
		label
			.get_as_mut::<WidgetLabel>()
			.unwrap()
			.set_text(common, Translation::from_raw_text(text));

		button.try_cast::<ComponentButton>()?.set_text(
			common,
			Translation::from_raw_text("this button has been clicked"),
		);

		Ok(())
	})
}

fn handle_button_click(button: Rc<ComponentButton>, label: Widget, text: &'static str) {
	button.on_click(button_click_callback(
		Component(button.clone()),
		label,
		text,
	));
}

impl TestbedGeneric {
	pub fn new(listeners: &mut EventListenerCollection<(), ()>) -> anyhow::Result<Self> {
		const XML_PATH: AssetPath = AssetPath::BuiltIn("gui/various_widgets.xml");

		let globals = WguiGlobals::new(
			Box::new(assets::Asset {}),
			wgui::globals::Defaults::default(),
		)?;

		let extra = ParseDocumentExtra {
			on_custom_attribs: Some(Box::new(move |par| {
				let Some(my_custom_value) = par.get_value("_my_custom") else {
					return;
				};

				let Some(mult_value) = par.get_value("_mult") else {
					return;
				};

				let mult_f32 = mult_value.parse::<f32>().unwrap();

				let mut color = match my_custom_value.as_ref() {
					"red" => Color::new(1.0, 0.0, 0.0, 1.0),
					"green" => Color::new(0.0, 1.0, 0.0, 1.0),
					"blue" => Color::new(0.0, 0.0, 1.0, 1.0),
					_ => Color::new(1.0, 1.0, 1.0, 1.0),
				};

				color = color.mult_rgb(mult_f32);

				let mut rect = par.get_widget_as::<WidgetRectangle>().unwrap();
				rect.params.color = color;
			})),
			dev_mode: false,
		};

		let (layout, state) = wgui::parser::new_layout_from_assets(
			listeners,
			&ParseDocumentParams {
				globals: globals.clone(),
				path: XML_PATH,
				extra,
			},
			&LayoutParams {
				resize_to_parent: true,
			},
		)?;

		let label_cur_option = state.fetch_widget(&layout.state, "label_current_option")?;

		let button_click_me = state.fetch_component_as::<ComponentButton>("button_click_me")?;
		let button = button_click_me.clone();
		button_click_me.on_click(Box::new(move |common, _e| {
			button.set_text(common, Translation::from_raw_text("congrats!"));
			Ok(())
		}));

		let button_popup = state.fetch_component_as::<ComponentButton>("button_popup")?;
		let button_red = state.fetch_component_as::<ComponentButton>("button_red")?;
		let button_aqua = state.fetch_component_as::<ComponentButton>("button_aqua")?;
		let button_yellow = state.fetch_component_as::<ComponentButton>("button_yellow")?;

		handle_button_click(button_red, label_cur_option.widget.clone(), "Clicked red");
		handle_button_click(button_aqua, label_cur_option.widget.clone(), "Clicked aqua");
		handle_button_click(
			button_yellow,
			label_cur_option.widget.clone(),
			"Clicked yellow",
		);

		let cb_first = state.fetch_component_as::<ComponentCheckbox>("cb_first")?;
		let label = label_cur_option.widget.clone();
		cb_first.on_toggle(Box::new(move |common, e| {
			let mut widget = label.get_as_mut::<WidgetLabel>().unwrap();
			let text = format!("checkbox toggle: {}", e.checked);
			widget.set_text(common, Translation::from_raw_text(&text));
			Ok(())
		}));

		let testbed = Self {
			layout: layout.as_rc(),
			globals: globals.clone(),
			data: Rc::new(RefCell::new(Data {
				state,
				tasks: Default::default(),
				popup_window: WguiWindow::default(),
			})),
		};

		button_popup.on_click({
			let testbed = testbed.clone();
			Box::new(move |_, _| {
				testbed.push_task(TestbedTask::ShowPopup);
				Ok(())
			})
		});

		Ok(testbed)
	}

	fn push_task(&self, task: TestbedTask) {
		self.data.borrow_mut().tasks.push_back(task);
	}

	fn process_task(
		&mut self,
		task: &TestbedTask,
		params: &mut TestbedUpdateParams,
		layout: &mut Layout,
		data: &mut Data,
	) -> anyhow::Result<()> {
		match task {
			TestbedTask::ShowPopup => self.show_popup(params, layout, data)?,
		}

		Ok(())
	}

	fn show_popup(
		&mut self,
		params: &mut TestbedUpdateParams,
		layout: &mut Layout,
		data: &mut Data,
	) -> anyhow::Result<()> {
		data.popup_window.open(WguiWindowParams {
			globals: self.globals.clone(),
			position: Vec2::new(128.0, 128.0),
			layout,
			listeners: params.listeners,
		})?;

		Ok(())
	}
}

impl Testbed for TestbedGeneric {
	fn update(&mut self, mut params: TestbedUpdateParams) -> anyhow::Result<()> {
		let layout = self.layout.clone();
		let data = self.data.clone();

		let mut layout = layout.borrow_mut();
		let mut data = data.borrow_mut();

		layout.update(
			Vec2::new(params.width, params.height),
			params.timestep_alpha,
		)?;

		while let Some(task) = data.tasks.pop_front() {
			self.process_task(&task, &mut params, &mut layout, &mut data)?;
		}

		Ok(())
	}

	fn layout(&self) -> &RcLayout {
		&self.layout
	}
}
