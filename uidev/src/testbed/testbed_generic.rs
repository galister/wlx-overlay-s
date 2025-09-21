use std::rc::Rc;

use crate::{
	assets,
	testbed::{Testbed, TestbedUpdateParams},
};
use glam::Vec2;
use wgui::{
	components::{
		Component,
		button::{ButtonClickCallback, ComponentButton},
		checkbox::ComponentCheckbox,
	},
	drawing::Color,
	event::EventListenerCollection,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{LayoutParams, RcLayout, Widget},
	parser::{ParseDocumentExtra, ParseDocumentParams, ParserState},
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

pub struct TestbedGeneric {
	pub layout: RcLayout,

	#[allow(dead_code)]
	state: ParserState,
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
		const XML_PATH: &str = "gui/various_widgets.xml";

		let globals = WguiGlobals::new(
			Box::new(assets::Asset {}),
			wgui::globals::Defaults::default(),
		)?;

		let extra = ParseDocumentExtra {
			on_custom_attribs: Some(Box::new(move |par| {
				let Some(my_custom_value) = par.get_value("my_custom") else {
					return;
				};

				let Some(mult_value) = par.get_value("mult") else {
					return;
				};

				let mult_f32 = mult_value.parse::<f32>().unwrap();

				let mut color = match my_custom_value {
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
				globals,
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

		Ok(Self {
			layout: layout.as_rc(),
			state,
		})
	}
}

impl Testbed for TestbedGeneric {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		self.layout.borrow_mut().update(
			Vec2::new(params.width, params.height),
			params.timestep_alpha,
		)?;
		Ok(())
	}

	fn layout(&self) -> &RcLayout {
		&self.layout
	}
}
