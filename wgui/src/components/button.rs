use std::sync::Arc;
use taffy::{AlignItems, JustifyContent, prelude::length};

use crate::{
	animation::{Animation, AnimationEasing},
	components::Component,
	drawing::{self, Color},
	event::WidgetCallback,
	layout::{Layout, WidgetID},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		rectangle::{Rectangle, RectangleParams},
		text::{TextLabel, TextParams},
		util::WLength,
	},
};

pub struct Params<'a> {
	pub text: &'a str,
	pub color: drawing::Color,
	pub border_color: drawing::Color,
	pub round: WLength,
	pub style: taffy::Style,
	pub text_style: TextStyle,
}

impl Default for Params<'_> {
	fn default() -> Self {
		Self {
			text: "Text",
			color: drawing::Color::new(1.0, 1.0, 1.0, 1.0),
			border_color: drawing::Color::new(0.0, 0.0, 0.0, 1.0),
			round: WLength::Units(4.0),
			style: Default::default(),
			text_style: TextStyle::default(),
		}
	}
}

pub struct Button {
	pub color: drawing::Color,
	pub body: WidgetID,    // Rectangle
	pub text_id: WidgetID, // Text
	text_node: taffy::NodeId,
}

impl Component for Button {}

impl Button {
	pub fn set_text<'a, C>(&self, callback_data: &mut C, text: &str)
	where
		C: WidgetCallback<'a>,
	{
		callback_data.call_on_widget(self.text_id, |label: &mut TextLabel| {
			label.set_text(text);
		});
		callback_data.mark_redraw();
		callback_data.mark_dirty(self.text_node);
	}
}

fn anim_hover_in(button: Arc<Button>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		10,
		AnimationEasing::OutQuad,
		Box::new(move |data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			let brightness = data.pos * 0.5;
			rect.params.color.r = button.color.r + brightness;
			rect.params.color.g = button.color.g + brightness;
			rect.params.color.b = button.color.b + brightness;
			rect.params.border_color = Color::new(1.0, 1.0, 1.0, 1.0);
			rect.params.border = 1.0 + data.pos;
			data.needs_redraw = true;
		}),
	)
}

fn anim_hover_out(button: Arc<Button>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		15,
		AnimationEasing::OutQuad,
		Box::new(move |data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			let brightness = (1.0 - data.pos) * 0.5;
			rect.params.color.r = button.color.r + brightness;
			rect.params.color.g = button.color.g + brightness;
			rect.params.color.b = button.color.b + brightness;
			rect.params.border_color = Color::new(1.0, 1.0, 1.0, 1.0);
			rect.params.border = 1.0 + (1.0 - data.pos) * 2.0;
			data.needs_redraw = true;
		}),
	)
}

pub fn construct(
	layout: &mut Layout,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<Arc<Button>> {
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.padding = length(1.0);

	let (rect_id, _) = layout.add_child(
		parent,
		Rectangle::create(RectangleParams {
			color: params.color,
			round: params.round,
			border_color: params.border_color,
			border: 2.0,
			..Default::default()
		})?,
		style,
	)?;

	let light_text = (params.color.r + params.color.g + params.color.b) < 1.5;

	let (text_id, text_node) = layout.add_child(
		rect_id,
		TextLabel::create(TextParams {
			content: String::from(params.text),
			style: TextStyle {
				weight: Some(FontWeight::Bold),
				color: Some(if light_text {
					Color::new(1.0, 1.0, 1.0, 1.0)
				} else {
					Color::new(0.0, 0.0, 0.0, 1.0)
				}),
				..params.text_style
			},
		})?,
		taffy::Style {
			..Default::default()
		},
	)?;

	let button = Arc::new(Button {
		body: rect_id,
		color: params.color,
		text_id,
		text_node,
	});

	//TODO: Highlight background on mouse enter

	//TODO: Bring back old color on mouse leave

	Ok(button)
}
