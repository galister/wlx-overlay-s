use std::{cell::RefCell, path::PathBuf, rc::Rc};

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
	event::StyleSetRequest,
	font_config::WguiFontConfig,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, LayoutParams, Widget},
	parser::{Fetchable, ParseDocumentExtra, ParseDocumentParams, ParserState},
	taffy,
	task::Tasks,
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
	windowing::{WguiWindow, WguiWindowParams},
};

pub enum TestbedTask {
	ShowPopup,
}

struct Data {
	#[allow(dead_code)]
	state: ParserState,

	popup_window: WguiWindow,
}

pub struct TestbedGeneric {
	pub layout: Layout,
	tasks: Tasks<TestbedTask>,

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
	pub fn new() -> anyhow::Result<Self> {
		const XML_PATH: AssetPath = AssetPath::BuiltIn("gui/various_widgets.xml");

		let globals = WguiGlobals::new(
			Box::new(assets::Asset {}),
			wgui::globals::Defaults::default(),
			&WguiFontConfig::default(),
			PathBuf::new(), // cwd
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
			&ParseDocumentParams {
				globals: globals.clone(),
				path: XML_PATH,
				extra,
			},
			&LayoutParams {
				resize_to_parent: true,
			},
		)?;

		let cb_visible = state.fetch_component_as::<ComponentCheckbox>("cb_visible")?;
		let div_visibility = state.fetch_widget(&layout.state, "div_visibility")?;

		cb_visible.on_toggle(Box::new(move |common, evt| {
			common.alterables.set_style(
				div_visibility.id,
				StyleSetRequest::Display(if evt.checked {
					taffy::Display::Flex
				} else {
					taffy::Display::None
				}),
			);
			Ok(())
		}));

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
			layout,
			tasks: Default::default(),
			globals: globals.clone(),
			data: Rc::new(RefCell::new(Data {
				state,
				popup_window: WguiWindow::default(),
			})),
		};

		button_popup.on_click({
			let tasks = testbed.tasks.clone();
			Box::new(move |_, _| {
				tasks.push(TestbedTask::ShowPopup);
				Ok(())
			})
		});

		Ok(testbed)
	}

	fn process_task(
		&mut self,
		task: &TestbedTask,
		params: &mut TestbedUpdateParams,
		data: &mut Data,
	) -> anyhow::Result<()> {
		match task {
			TestbedTask::ShowPopup => self.show_popup(params, data)?,
		}

		Ok(())
	}

	fn show_popup(
		&mut self,
		_params: &mut TestbedUpdateParams,
		data: &mut Data,
	) -> anyhow::Result<()> {
		data.popup_window.open(&mut WguiWindowParams {
			globals: self.globals.clone(),
			position: Vec2::new(128.0, 128.0),
			layout: &mut self.layout,
			title: Translation::from_raw_text("foo"),
			extra: Default::default(),
		})?;

		Ok(())
	}
}

impl Testbed for TestbedGeneric {
	fn update(&mut self, mut params: TestbedUpdateParams) -> anyhow::Result<()> {
		let data = self.data.clone();
		let mut data = data.borrow_mut();

		self.layout.update(
			Vec2::new(params.width, params.height),
			params.timestep_alpha,
		)?;

		for task in self.tasks.drain() {
			self.process_task(&task, &mut params, &mut data)?;
		}

		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.layout
	}
}
