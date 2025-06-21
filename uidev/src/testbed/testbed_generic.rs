use std::{cell::RefCell, rc::Rc};

use glam::{Mat4, Vec2};
use wgui::{
	drawing::{self},
	event::EventListener,
	layout::{Layout, WidgetID},
	renderer_vk::text::TextStyle,
};

use crate::{assets, testbed::Testbed};

pub struct TestbedGeneric {
	pub layout: Layout,
	rot: f32,
	widget_id: Rc<RefCell<Option<WidgetID>>>,
}

impl TestbedGeneric {
	pub fn new() -> anyhow::Result<Self> {
		const XML_PATH: &str = "gui/testbed.xml";

		let (mut layout, res) =
			wgui::parser::new_layout_from_assets(Box::new(assets::Asset {}), XML_PATH)?;

		use wgui::components::button;
		let my_div_parent = res.require_by_id("my_div_parent")?;
		// create some buttons for testing
		for i in 0..4 {
			let n = i as f32 / 4.0;
			button::construct(
				&mut layout,
				my_div_parent,
				button::Params {
					text: "I'm a button!",
					color: drawing::Color::new(1.0 - n, n * n, n, 1.0),
					..Default::default()
				},
			)?;
		}

		let button = button::construct(
			&mut layout,
			my_div_parent,
			button::Params {
				text: "Click me!!",
				color: drawing::Color::new(0.2, 0.2, 0.2, 1.0),
				size: Vec2::new(256.0, 64.0),
				text_style: TextStyle {
					size: Some(30.0),
					..Default::default()
				},
			},
		)?;

		let widget_id = Rc::new(RefCell::new(None));

		let wid = widget_id.clone();
		layout.add_event_listener(
			button.body,
			EventListener::MouseRelease(Box::new(move |data, _| {
				button.set_text(data, "Congratulations!");
				*wid.borrow_mut() = Some(data.widget_id);
			})),
		);

		Ok(Self {
			layout,
			rot: 0.0,
			widget_id,
		})
	}
}

impl Testbed for TestbedGeneric {
	fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		if let Some(widget_id) = *self.widget_id.borrow() {
			self.rot += 0.01;

			let a = self.layout.widget_map.get(widget_id).unwrap();
			let mut widget = a.lock().unwrap();
			widget.data.transform = Mat4::IDENTITY
				* Mat4::from_rotation_y(-self.rot)
				* Mat4::from_rotation_x(self.rot * 0.25)
				* Mat4::from_rotation_z(-self.rot * 0.1);

			self.layout.needs_redraw = true;
		}

		self
			.layout
			.update(Vec2::new(width, height), timestep_alpha)?;
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.layout
	}
}