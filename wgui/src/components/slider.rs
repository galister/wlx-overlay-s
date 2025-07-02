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
	event::{self, CallbackDataCommon, EventListenerCollection, EventListenerKind},
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

pub struct Slider {
	body: WidgetID,                  // Div
	slider_handle_id: WidgetID,      // Div
	slider_handle_rect_id: WidgetID, // Rectangle
	slider_handle_node: taffy::NodeId,
	state: Rc<RefCell<SliderState>>,
}

impl Component for Slider {}

impl Slider {
	fn get_state(&self) -> RefMut<'_, SliderState> {
		self.state.borrow_mut()
	}

	pub fn set_value(&self, state: &mut SliderState, common: &mut CallbackDataCommon, value: f32) {
		state.value = value;

		common.mark_dirty(self.slider_handle_node);

		common.call_on_widget(self.slider_handle_id, |div: &mut Div| {});
		common.mark_redraw();
		common.mark_dirty(self.slider_handle_node);
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

fn register_event_mouse_enter<U1, U2>(
	slider: Rc<Slider>,
	listeners: &mut EventListenerCollection<U1, U2>,
) {
	listeners.add(
		slider.body,
		EventListenerKind::MouseEnter,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();
			slider.get_state().hovered = true;
			on_enter_anim(common, slider.slider_handle_rect_id);
		}),
	);
}

fn register_event_mouse_leave<U1, U2>(
	slider: Rc<Slider>,
	listeners: &mut EventListenerCollection<U1, U2>,
) {
	listeners.add(
		slider.body,
		EventListenerKind::MouseLeave,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();
			slider.get_state().hovered = false;
			on_leave_anim(common, slider.slider_handle_rect_id);
		}),
	);
}

fn register_event_mouse_motion<U1, U2>(
	slider: Rc<Slider>,
	listeners: &mut EventListenerCollection<U1, U2>,
) {
	listeners.add(
		slider.body,
		EventListenerKind::MouseMotion,
		Box::new(move |_common, _data, _, _| {}),
	);
}

fn register_event_mouse_press<U1, U2>(
	slider: Rc<Slider>,
	listeners: &mut EventListenerCollection<U1, U2>,
) {
	listeners.add(
		slider.body,
		EventListenerKind::MousePress,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();

			let mut state = slider.get_state();
			if state.hovered {
				state.dragging = true;
				let val = state.min_value;
				slider.set_value(&mut state, common, val);
			}
		}),
	);
}

fn register_event_mouse_release<U1, U2>(
	slider: Rc<Slider>,
	listeners: &mut EventListenerCollection<U1, U2>,
) {
	listeners.add(
		slider.body,
		EventListenerKind::MouseRelease,
		Box::new(move |common, _data, _, _| {
			common.trigger_haptics();

			let mut state = slider.get_state();
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

	let slider = Rc::new(Slider {
		body: body_id,
		slider_handle_node,
		slider_handle_rect_id,
		slider_handle_id,
		state: Rc::new(RefCell::new(SliderState {
			dragging: false,
			hovered: false,
			max_value: params.max_value,
			value: params.initial_value,
			min_value: params.min_value,
		})),
	});

	register_event_mouse_enter(slider.clone(), listeners);
	register_event_mouse_leave(slider.clone(), listeners);
	register_event_mouse_motion(slider.clone(), listeners);
	register_event_mouse_press(slider.clone(), listeners);
	register_event_mouse_leave(slider.clone(), listeners);
	register_event_mouse_release(slider.clone(), listeners);

	Ok(slider)
}
