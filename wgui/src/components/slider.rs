use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};
use taffy::prelude::{length, percent};

use crate::{
	animation::{Animation, AnimationEasing},
	components::Component,
	drawing::{self},
	event::{self, EventListenerCollection, EventListenerKind, WidgetCallback},
	layout::{Layout, WidgetID},
	renderer_vk::util,
	widget::{
		div::Div,
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

const BODY_COLOR: drawing::Color = drawing::Color::new(0.6, 0.65, 0.7, 1.0);
const BODY_BORDER_COLOR: drawing::Color = drawing::Color::new(0.4, 0.45, 0.5, 1.0);
const HANDLE_BORDER_COLOR: drawing::Color = drawing::Color::new(0.85, 0.85, 0.85, 1.0);
const HANDLE_BORDER_COLOR_HOVERED: drawing::Color = drawing::Color::new(0.0, 0.0, 0.0, 1.0);
const HANDLE_COLOR: drawing::Color = drawing::Color::new(1.0, 1.0, 1.0, 1.0);
const HANDLE_COLOR_HOVERED: drawing::Color = drawing::Color::new(0.9, 0.9, 0.9, 1.0);

const SLIDER_HOVER_SCALE: f32 = 0.25;
fn get_anim_transform(pos: f32, widget_size: Vec2) -> Mat4 {
	util::centered_matrix(
		widget_size,
		&Mat4::from_scale(Vec3::splat(SLIDER_HOVER_SCALE.mul_add(pos, 1.0))),
	)
}

fn anim_rect(rect: &mut Rectangle, pos: f32) {
	rect.params.color = drawing::Color::lerp(&HANDLE_COLOR, &HANDLE_COLOR_HOVERED, pos);
	rect.params.border_color =
		drawing::Color::lerp(&HANDLE_BORDER_COLOR, &HANDLE_BORDER_COLOR_HOVERED, pos);
}

fn on_enter_anim(data: &mut event::CallbackData, handle_id: WidgetID) {
	data.animations.push(Animation::new(
		handle_id,
		5,
		AnimationEasing::OutQuad,
		Box::new(move |data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			data.data.transform = get_anim_transform(data.pos, data.widget_size);
			anim_rect(rect, data.pos);
			data.needs_redraw = true;
		}),
	));
}

fn on_leave_anim(data: &mut event::CallbackData, handle_id: WidgetID) {
	data.animations.push(Animation::new(
		handle_id,
		10,
		AnimationEasing::OutQuad,
		Box::new(move |data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			data.data.transform = get_anim_transform(1.0 - data.pos, data.widget_size);
			anim_rect(rect, 1.0 - data.pos);
			data.needs_redraw = true;
		}),
	));
}

const PAD_PERCENT: f32 = 0.75;

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
	style.align_items = Some(taffy::AlignItems::Center);
	style.justify_content = Some(taffy::JustifyContent::Center);

	let (body_id, _) = layout.add_child(parent, Div::create()?, style)?;

	let (_background_id, _) = layout.add_child(
		body_id,
		Rectangle::create(RectangleParams {
			color: BODY_COLOR,
			round: WLength::Percent(1.0),
			border_color: BODY_BORDER_COLOR,
			border: 2.0,
			..Default::default()
		})?,
		taffy::Style {
			size: taffy::Size {
				width: percent(1.0),
				height: percent(PAD_PERCENT),
			},
			position: taffy::Position::Absolute,
			..Default::default()
		},
	)?;

	let mut handle_style = taffy::Style::default();
	handle_style.size.width = length(32.0);
	handle_style.size.height = percent(1.0);
	handle_style.position = taffy::Position::Absolute;
	handle_style.align_items = Some(taffy::AlignItems::Center);
	handle_style.justify_content = Some(taffy::JustifyContent::Center);

	// invisible outer handle body
	let (slider_handle_id, slider_handle_node) =
		layout.add_child(body_id, Div::create()?, handle_style)?;

	let (slider_handle_rect_id, _) = layout.add_child(
		slider_handle_id,
		Rectangle::create(RectangleParams {
			color: HANDLE_COLOR,
			border_color: HANDLE_BORDER_COLOR,
			border: 2.0,
			round: WLength::Percent(1.0),
			..Default::default()
		})?,
		taffy::Style {
			size: taffy::Size {
				width: percent(PAD_PERCENT),
				height: percent(PAD_PERCENT),
			},
			..Default::default()
		},
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
		Box::new(move |data, _, _| {
			data.trigger_haptics = true;
			on_enter_anim(data, slider_handle_rect_id);
		}),
	);

	listeners.add(
		body_id,
		EventListenerKind::MouseMotion,
		Box::new(move |_data, _, _| {}),
	);

	listeners.add(
		body_id,
		EventListenerKind::MouseLeave,
		Box::new(move |data, _, _| {
			data.trigger_haptics = true;
			on_leave_anim(data, slider_handle_rect_id);
		}),
	);

	Ok(slider)
}
