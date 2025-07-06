use std::rc::Rc;
use taffy::{AlignItems, JustifyContent, prelude::length};

use crate::{
	animation::{Animation, AnimationEasing},
	components::Component,
	drawing::{self, Color},
	event::{CallbackDataCommon, EventListenerCollection, EventListenerKind, ListenerHandleVec},
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

struct Data {
	initial_color: drawing::Color,
	initial_border_color: drawing::Color,
	text_id: WidgetID, // Text
	text_node: taffy::NodeId,
}

pub struct Button {
	data: Rc<Data>,
	#[allow(dead_code)]
	listener_handles: ListenerHandleVec,
}

impl Component for Button {}

impl Button {
	pub fn set_text<C>(&self, callback_data: &mut CallbackDataCommon, text: &str) {
		callback_data.call_on_widget(self.data.text_id, |label: &mut TextLabel| {
			label.set_text(text);
		});
		callback_data.mark_redraw();
		callback_data.mark_dirty(self.data.text_node);
	}
}

fn anim_hover(rect: &mut Rectangle, data: &Data, pos: f32) {
	let brightness = pos * 0.5;
	let border_brightness = pos;
	rect.params.color.r = data.initial_color.r + brightness;
	rect.params.color.g = data.initial_color.g + brightness;
	rect.params.color.b = data.initial_color.b + brightness;
	rect.params.border_color.r = data.initial_border_color.r + border_brightness;
	rect.params.border_color.g = data.initial_border_color.g + border_brightness;
	rect.params.border_color.b = data.initial_border_color.b + border_brightness;
	rect.params.border = 3.0;
}

fn anim_hover_in(data: Rc<Data>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		10,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<Rectangle>();
			anim_hover(rect, &data, anim_data.pos);
			common.mark_redraw();
		}),
	)
}

fn anim_hover_out(data: Rc<Data>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		15,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<Rectangle>();
			anim_hover(rect, &data, 1.0 - anim_data.pos);
			common.mark_redraw();
		}),
	)
}

pub fn construct<U1, U2>(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<Rc<Button>> {
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.padding = length(1.0);

	let (rect_id, _) = layout.add_child(
		parent,
		Rectangle::create(RectangleParams {
			color: params.color,
			color2: params
				.color
				.lerp(&Color::new(0.0, 0.0, 0.0, params.color.a), 0.3),
			gradient: drawing::GradientMode::Vertical,
			round: params.round,
			border_color: params.border_color,
			border: 2.0,
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

	let _data = Rc::new(Data {
		text_id,
		text_node,
		initial_color: params.color,
		initial_border_color: params.border_color,
	});

	let mut listener_handles = ListenerHandleVec::default();

	let data = _data.clone();
	listeners.register(
		&mut listener_handles,
		rect_id,
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, _, _| {
			common.animate(anim_hover_in(data.clone(), event_data.widget_id));
		}),
	);

	let data = _data.clone();
	listeners.register(
		&mut listener_handles,
		rect_id,
		EventListenerKind::MouseLeave,
		Box::new(move |common, event_data, _, _| {
			common.animate(anim_hover_out(data.clone(), event_data.widget_id));
		}),
	);

	Ok(Rc::new(Button {
		data: _data.clone(),
		listener_handles,
	}))
}
