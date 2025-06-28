use std::sync::Arc;

use taffy::prelude::{length, percent};

use crate::{
	components::Component,
	drawing::{self},
	event::{EventListenerCollection, EventListenerKind, WidgetCallback},
	layout::{Layout, WidgetID},
	widget::{
		rectangle::{Rectangle, RectangleParams},
		util::WLength,
	},
};

pub struct Params {
	pub style: taffy::Style,
	pub initial_value: f32,
	pub min_value: f32,
	pub max_value: f32,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			style: Default::default(),
			initial_value: 0.5,
			min_value: 0.0,
			max_value: 1.0,
		}
	}
}

pub struct Slider {
	pub body: WidgetID,             // Outer rectangle
	pub slider_handle_id: WidgetID, // Inner rectangle
	pub slider_handle_node: taffy::NodeId,
}

impl Component for Slider {}

impl Slider {
	pub fn set_value<'a, C>(&self, callback_data: &mut C, _value: f32)
	where
		C: WidgetCallback<'a>,
	{
		callback_data.mark_redraw();
		callback_data.mark_dirty(self.slider_handle_node);
		callback_data.call_on_widget(self.slider_handle_id, |_rect: &mut Rectangle| {
			// todo
		});
		callback_data.mark_redraw();
		callback_data.mark_dirty(self.slider_handle_node);
	}
}

pub fn construct<U1, U2>(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<Arc<Slider>> {
	let mut style = params.style;
	style.position = taffy::Position::Relative;
	style.min_size = style.size;
	style.max_size = style.size;

	let body_color = drawing::Color::new(0.2, 0.3, 0.4, 1.0);
	let body_border_color = drawing::Color::new(0.1, 0.2, 0.3, 1.0);
	let handle_color = drawing::Color::new(1.0, 1.0, 1.0, 1.0);

	let (body_id, _) = layout.add_child(
		parent,
		Rectangle::create(RectangleParams {
			color: body_color,
			round: WLength::Percent(1.0),
			border_color: body_border_color,
			border: 2.0,
			..Default::default()
		})?,
		style,
	)?;

	let mut handle_style = taffy::Style::default();
	handle_style.size.width = length(32.0);
	handle_style.size.height = percent(1.0);

	let (slider_handle_id, slider_handle_node) = layout.add_child(
		body_id,
		Rectangle::create(RectangleParams {
			color: handle_color,
			round: WLength::Percent(1.0),
			..Default::default()
		})?,
		handle_style,
	)?;

	let slider = Arc::new(Slider {
		body: body_id,
		slider_handle_node,
		slider_handle_id,
	});

	//let mut widget = layout.widget_map.get(rect_id).unwrap().lock().unwrap();

	listeners.add(
		body_id,
		EventListenerKind::MouseEnter,
		Box::new(move |_data, _, _| {}),
	);

	listeners.add(
		body_id,
		EventListenerKind::MouseMotion,
		Box::new(move |_data, _, _| {}),
	);

	listeners.add(
		body_id,
		EventListenerKind::MouseLeave,
		Box::new(move |_data, _, _| {}),
	);

	Ok(slider)
}
