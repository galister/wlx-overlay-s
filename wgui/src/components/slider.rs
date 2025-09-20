use std::{cell::RefCell, rc::Rc};

use glam::{Mat4, Vec2, Vec3};
use taffy::prelude::{length, percent};

use crate::{
	animation::{Animation, AnimationEasing},
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::{self},
	event::{self, CallbackDataCommon, EventListenerCollection, EventListenerKind, ListenerHandleVec},
	i18n::Translation,
	layout::{Layout, WidgetID},
	renderer_vk::{
		text::{FontWeight, HorizontalAlign, TextStyle},
		util,
	},
	widget::{
		div::WidgetDiv,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

#[derive(Default)]
pub struct ValuesMinMax {
	pub value: f32,
	pub min_value: f32,
	pub max_value: f32,
}

impl ValuesMinMax {
	fn to_normalized(&self) -> f32 {
		(self.value - self.min_value) / (self.max_value - self.min_value)
	}

	fn get_from_normalized(&self, normalized: f32) -> f32 {
		normalized * (self.max_value - self.min_value) + self.min_value
	}
}

#[derive(Default)]
pub struct Params {
	pub style: taffy::Style,
	pub values: ValuesMinMax,
}

struct State {
	dragging: bool,
	hovered: bool,
	values: ValuesMinMax,
}

struct Data {
	body: WidgetID,                  // Div
	slider_handle_rect_id: WidgetID, // Rectangle
	slider_text_id: WidgetID,        // Text
	slider_handle_node: taffy::NodeId,
	slider_body_node: taffy::NodeId,
}

pub struct ComponentSlider {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
}

impl ComponentTrait for ComponentSlider {
	fn init(&self, init_data: &mut InitData) {
		let mut state = self.state.borrow_mut();
		let value = state.values.value;
		state.set_value(init_data.common, &self.data, value);
	}

	fn base(&mut self) -> &mut ComponentBase {
		&mut self.base
	}
}

// NOTICE: this can be re-used in the future
fn map_mouse_x_to_normalized(mouse_x_rel: f32, widget_width: f32) -> f32 {
	(mouse_x_rel / widget_width).clamp(0.0, 1.0)
}

fn get_width(slider_body_node: taffy::NodeId, tree: &taffy::tree::TaffyTree<WidgetID>) -> f32 {
	let layout = tree.layout(slider_body_node).unwrap(); /* shouldn't fail */
	layout.size.width
}

fn conf_handle_style(
	values: &ValuesMinMax,
	slider_body_node: taffy::NodeId,
	slider_handle_style: &mut taffy::Style,
	tree: &taffy::tree::TaffyTree<WidgetID>,
) {
	let norm = values.to_normalized();

	// convert normalized value to taffy percentage margin in percent
	let width = get_width(slider_body_node, tree);
	let percent_margin = (HANDLE_WIDTH / width) / 2.0;
	slider_handle_style.margin.left = percent(percent_margin + norm * (1.0 - percent_margin * 2.0));
}

const PAD_PERCENT: f32 = 0.75;
const HANDLE_WIDTH: f32 = 32.0;
const HANDLE_HEIGHT: f32 = 24.0;

impl State {
	fn update_value_to_mouse(
		&mut self,
		event_data: &event::CallbackData<'_>,
		data: &Data,
		common: &mut CallbackDataCommon,
	) {
		let mouse_pos = event_data
			.metadata
			.get_mouse_pos_relative(&common.alterables.transform_stack)
			.unwrap(); // safe

		let norm = map_mouse_x_to_normalized(
			mouse_pos.x - HANDLE_WIDTH / 2.0,
			get_width(data.slider_body_node, &common.state.tree) - HANDLE_WIDTH,
		);

		let target_value = self.values.get_from_normalized(norm);
		let val = target_value;

		self.set_value(common, data, val);
	}

	fn update_text(common: &mut CallbackDataCommon, text: &mut WidgetLabel, value: f32) {
		// round displayed value, should be sufficient for now
		text.set_text(common, Translation::from_raw_text(&format!("{}", value.round())));
	}

	fn set_value(&mut self, common: &mut CallbackDataCommon, data: &Data, value: f32) {
		//common.call_on_widget(data.slider_handle_id, |_div: &mut Div| {});
		self.values.value = value;
		let mut style = common.state.tree.style(data.slider_handle_node).unwrap().clone();
		conf_handle_style(&self.values, data.slider_body_node, &mut style, &common.state.tree);
		common.alterables.mark_dirty(data.slider_handle_node);
		common.alterables.mark_redraw();
		common.alterables.set_style(data.slider_handle_node, style);

		if let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(data.slider_text_id) {
			Self::update_text(common, &mut label, value);
		}
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

fn anim_rect(rect: &mut WidgetRectangle, pos: f32) {
	rect.params.color = drawing::Color::lerp(&HANDLE_COLOR, &HANDLE_COLOR_HOVERED, pos);
	rect.params.border_color = drawing::Color::lerp(&HANDLE_BORDER_COLOR, &HANDLE_BORDER_COLOR_HOVERED, pos);
}

fn on_enter_anim(common: &mut event::CallbackDataCommon, handle_id: WidgetID) {
	common.alterables.animate(Animation::new(
		handle_id,
		20,
		AnimationEasing::OutBack,
		Box::new(move |common, data| {
			let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			data.data.transform = get_anim_transform(data.pos, data.widget_size);
			anim_rect(rect, data.pos);
			common.alterables.mark_redraw();
		}),
	));
}

fn on_leave_anim(common: &mut event::CallbackDataCommon, handle_id: WidgetID) {
	common.alterables.animate(Animation::new(
		handle_id,
		10,
		AnimationEasing::OutQuad,
		Box::new(move |common, data| {
			let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			data.data.transform = get_anim_transform(1.0 - data.pos, data.widget_size);
			anim_rect(rect, 1.0 - data.pos);
			common.alterables.mark_redraw();
		}),
	));
}

fn register_event_mouse_enter<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseEnter,
		Box::new(move |common, _data, _, _| {
			common.alterables.trigger_haptics();
			state.borrow_mut().hovered = true;
			on_enter_anim(common, data.slider_handle_rect_id);
			Ok(())
		}),
	);
}

fn register_event_mouse_leave<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseLeave,
		Box::new(move |common, _data, _, _| {
			common.alterables.trigger_haptics();
			state.borrow_mut().hovered = false;
			on_leave_anim(common, data.slider_handle_rect_id);
			Ok(())
		}),
	);
}

fn register_event_mouse_motion<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseMotion,
		Box::new(move |common, event_data, _, _| {
			let mut state = state.borrow_mut();

			if state.dragging {
				state.update_value_to_mouse(event_data, &data, common);
			}

			Ok(())
		}),
	);
}

fn register_event_mouse_press<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MousePress,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			let mut state = state.borrow_mut();

			if state.hovered {
				state.dragging = true;
				state.update_value_to_mouse(event_data, &data, common);
			}

			Ok(())
		}),
	);
}

fn register_event_mouse_release<U1, U2>(
	data: &Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.body,
		EventListenerKind::MouseRelease,
		Box::new(move |common, _data, _, _| {
			common.alterables.trigger_haptics();

			let mut state = state.borrow_mut();
			if state.dragging {
				state.dragging = false;
			}

			Ok(())
		}),
	);
}

pub fn construct<U1, U2>(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<(WidgetID, Rc<ComponentSlider>)> {
	let mut style = params.style;
	style.position = taffy::Position::Relative;
	style.min_size = style.size;
	style.max_size = style.size;

	let (root_id, slider_body_node) = layout.add_child(parent, WidgetDiv::create(), style)?;
	let body_id = root_id;

	let (_background_id, _) = layout.add_child(
		body_id,
		WidgetRectangle::create(WidgetRectangleParams {
			color: BODY_COLOR,
			round: WLength::Percent(1.0),
			border_color: BODY_BORDER_COLOR,
			border: 2.0,
			..Default::default()
		}),
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

	let slider_handle_style = taffy::Style {
		size: taffy::Size {
			width: length(0.0),
			height: percent(1.0),
		},
		position: taffy::Position::Absolute,
		align_items: Some(taffy::AlignItems::Center),
		justify_content: Some(taffy::JustifyContent::Center),
		..Default::default()
	};

	// invisible outer handle body
	let (slider_handle_id, slider_handle_node) = layout.add_child(body_id, WidgetDiv::create(), slider_handle_style)?;

	let (slider_handle_rect_id, _) = layout.add_child(
		slider_handle_id,
		WidgetRectangle::create(WidgetRectangleParams {
			color: HANDLE_COLOR,
			border_color: HANDLE_BORDER_COLOR,
			border: 2.0,
			round: WLength::Percent(1.0),
			..Default::default()
		}),
		taffy::Style {
			position: taffy::Position::Absolute,
			size: taffy::Size {
				width: length(HANDLE_WIDTH),
				height: length(HANDLE_HEIGHT),
			},
			..Default::default()
		},
	)?;

	let state = State {
		dragging: false,
		hovered: false,
		values: params.values,
	};

	let globals = layout.state.globals.clone();

	let (slider_text_id, _) = layout.add_child(
		slider_handle_id,
		WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: Translation::default(),
				style: TextStyle {
					color: Some(drawing::Color::new(0.0, 0.0, 0.0, 0.75)), // always black
					weight: Some(FontWeight::Bold),
					align: Some(HorizontalAlign::Center),
					..Default::default()
				},
			},
		),
		Default::default(),
	)?;

	let data = Rc::new(Data {
		body: body_id,
		slider_handle_node,
		slider_handle_rect_id,
		slider_body_node,
		slider_text_id,
	});

	let state = Rc::new(RefCell::new(state));

	let mut base = ComponentBase::default();

	register_event_mouse_enter(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_leave(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_motion(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_press(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_leave(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_release(&data, state.clone(), listeners, &mut base.lhandles);

	let slider = Rc::new(ComponentSlider { base, data, state });

	layout.defer_component_init(Component(slider.clone()));
	Ok((root_id, slider))
}
