use std::{
	cell::{RefCell, RefMut},
	rc::Rc,
};

use glam::{Mat4, Vec2, Vec3};
use taffy::prelude::{length, percent};

use crate::{
	animation::{Animation, AnimationEasing},
	components::Component,
	drawing::{self},
	event::{
		self, CallbackDataCommon, EventListenerCollection, EventListenerKind, ListenerHandleVec,
	},
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

pub struct SliderState {
	dragging: bool,
	hovered: bool,
	value: f32,
	min_value: f32,
	max_value: f32,
}

struct Data {
	body: WidgetID,                  // Div
	slider_handle_id: WidgetID,      // Div
	slider_handle_rect_id: WidgetID, // Rectangle
	slider_handle_node: taffy::NodeId,
}

pub struct Slider {
	data: Rc<Data>,
	state: Rc<RefCell<SliderState>>,
	listener_handles: ListenerHandleVec,
}

impl Component for Slider {}

impl SliderState {
	fn set_value(&mut self, data: &Data, common: &mut CallbackDataCommon, value: f32) {
		self.value = value;
		common.mark_dirty(data.slider_handle_node);
		common.call_on_widget(data.slider_handle_id, |div: &mut Div| {});
		common.mark_redraw();

		let mut style = common
			.refs
			.tree
			.style(data.slider_handle_node)
			.unwrap()
			.clone();

		// todo
		style.margin.left = percent(1.0);

		common.set_style(data.slider_handle_node, style);
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

fn on_enter_anim(common: &mut event::CallbackDataCommon, handle_id: WidgetID) {
	common.animate(Animation::new(
		handle_id,
		5,
		AnimationEasing::OutQuad,
		Box::new(move |common, data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			data.data.transform = get_anim_transform(data.pos, data.widget_size);
			anim_rect(rect, data.pos);
			common.mark_redraw();
		}),
	));
}

fn on_leave_anim(common: &mut event::CallbackDataCommon, handle_id: WidgetID) {
	common.animate(Animation::new(
		handle_id,
		10,
		AnimationEasing::OutQuad,
		Box::new(move |common, data| {
			let rect = data.obj.get_as_mut::<Rectangle>();
			data.data.transform = get_anim_transform(1.0 - data.pos, data.widget_size);
			anim_rect(rect, 1.0 - data.pos);
			common.mark_redraw();
		}),
	));
}

const PAD_PERCENT: f32 = 0.75;

const HANDLE_WIDTH: f32 = 32.0;
const HANDLE_HEIGHT: f32 = 24.0;

fn register_event_mouse_enter<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<SliderState>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseEnter,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();
			state.borrow_mut().hovered = true;
			on_enter_anim(common, data.slider_handle_rect_id);
		}),
	);
}

fn register_event_mouse_leave<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<SliderState>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseLeave,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();
			state.borrow_mut().hovered = false;
			on_leave_anim(common, data.slider_handle_rect_id);
		}),
	);
}

fn register_event_mouse_motion<U1, U2>(
	data: Rc<Data>,
	_state: Rc<RefCell<SliderState>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseMotion,
		Box::new(move |_common, _data, _, _| {}),
	);
}

fn register_event_mouse_press<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<SliderState>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MousePress,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();

			let mut state = state.borrow_mut();

			if state.hovered {
				state.dragging = true;
				let val = 1.0;
				state.set_value(&data, common, val);
			}
		}),
	);
}

fn register_event_mouse_release<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<SliderState>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseRelease,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();

			let mut state = state.borrow_mut();
			if state.dragging {
				state.dragging = false;
			}
		}),
	);
}

pub fn construct<U1, U2>(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<Rc<Slider>> {
	let mut style = params.style;
	style.position = taffy::Position::Relative;
	style.min_size = style.size;
	style.max_size = style.size;

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
			align_self: Some(taffy::AlignItems::Center),
			justify_self: Some(taffy::JustifySelf::Center),
			..Default::default()
		},
	)?;

	// invisible outer handle body
	let (slider_handle_id, slider_handle_node) = layout.add_child(
		body_id,
		Div::create()?,
		taffy::Style {
			size: taffy::Size {
				width: length(0.0),
				height: percent(1.0),
			},
			position: taffy::Position::Absolute,
			align_items: Some(taffy::AlignItems::Center),
			justify_content: Some(taffy::JustifyContent::Center),
			..Default::default()
		},
	)?;

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
			position: taffy::Position::Absolute,
			size: taffy::Size {
				width: length(HANDLE_WIDTH),
				height: length(HANDLE_HEIGHT),
			},
			..Default::default()
		},
	)?;

	let data = Rc::new(Data {
		body: body_id,
		slider_handle_node,
		slider_handle_rect_id,
		slider_handle_id,
	});

	let state = Rc::new(RefCell::new(SliderState {
		dragging: false,
		hovered: false,
		max_value: params.max_value,
		value: params.initial_value,
		min_value: params.min_value,
	}));

	let mut lhandles = ListenerHandleVec::default();

	register_event_mouse_enter(data.clone(), state.clone(), listeners, &mut lhandles);
	register_event_mouse_leave(data.clone(), state.clone(), listeners, &mut lhandles);
	register_event_mouse_motion(data.clone(), state.clone(), listeners, &mut lhandles);
	register_event_mouse_press(data.clone(), state.clone(), listeners, &mut lhandles);
	register_event_mouse_leave(data.clone(), state.clone(), listeners, &mut lhandles);
	register_event_mouse_release(data.clone(), state.clone(), listeners, &mut lhandles);

	let slider = Rc::new(Slider {
		data,
		state,
		listener_handles: lhandles,
	});

	Ok(slider)
}
