use std::{cell::RefCell, rc::Rc};
use taffy::{AlignItems, JustifyContent, prelude::length};

use crate::{
	animation::{Animation, AnimationEasing},
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::{self, Color},
	event::{EventAlterables, EventListenerCollection, EventListenerKind, ListenerHandleVec},
	i18n::Translation,
	layout::{Layout, LayoutState, WidgetID},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

pub struct Params {
	pub text: Translation,
	pub color: drawing::Color,
	pub border_color: drawing::Color,
	pub round: WLength,
	pub style: taffy::Style,
	pub text_style: TextStyle,
}

fn get_color2(color: &drawing::Color) -> drawing::Color {
	color.lerp(&Color::new(0.0, 0.0, 0.0, color.a), 0.2)
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Translation::from_raw_text(""),
			color: drawing::Color::new(1.0, 1.0, 1.0, 1.0),
			border_color: drawing::Color::new(0.0, 0.0, 0.0, 1.0),
			round: WLength::Units(4.0),
			style: Default::default(),
			text_style: TextStyle::default(),
		}
	}
}

pub struct ButtonClickEvent<'a> {
	pub state: &'a LayoutState,
	pub alterables: &'a mut EventAlterables,
}
pub type ButtonClickCallback = Box<dyn Fn(ButtonClickEvent) -> anyhow::Result<()>>;

struct State {
	hovered: bool,
	down: bool,
	on_click: Option<ButtonClickCallback>,
}

struct Data {
	initial_color: drawing::Color,
	initial_color2: drawing::Color,
	initial_border_color: drawing::Color,
	text_id: WidgetID, // Text
	rect_id: WidgetID, // Rectangle
	text_node: taffy::NodeId,
}

pub struct ComponentButton {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
}

impl ComponentTrait for ComponentButton {
	fn base(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn init(&self, _data: &mut InitData) {}
}

impl ComponentButton {
	pub fn set_text(&self, state: &LayoutState, alterables: &mut EventAlterables, text: Translation) {
		let globals = state.globals.clone();

		state
			.widgets
			.call(self.data.text_id, |label: &mut WidgetLabel| {
				label.set_text(&mut globals.i18n(), text);
			});

		alterables.mark_redraw();
		alterables.mark_dirty(self.data.text_node);
	}

	pub fn on_click(&self, func: ButtonClickCallback) {
		self.state.borrow_mut().on_click = Some(func);
	}
}

fn anim_hover(rect: &mut WidgetRectangle, data: &Data, pos: f32, pressed: bool) {
	let brightness = pos * if pressed { 0.75 } else { 0.5 };
	let border_brightness = pos;
	rect.params.color = data.initial_color.add_rgb(brightness);
	rect.params.color2 = data.initial_color2.add_rgb(brightness);
	rect.params.border_color = data.initial_border_color.add_rgb(border_brightness);
	rect.params.border = 2.0;
}

fn anim_hover_in(data: Rc<Data>, state: Rc<RefCell<State>>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		5,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>();
			anim_hover(rect, &data, anim_data.pos, state.borrow().down);
			common.alterables.mark_redraw();
		}),
	)
}

fn anim_hover_out(data: Rc<Data>, state: Rc<RefCell<State>>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		8,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>();
			anim_hover(rect, &data, 1.0 - anim_data.pos, state.borrow().down);
			common.alterables.mark_redraw();
		}),
	)
}

fn register_event_mouse_enter<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.rect_id,
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common.alterables.animate(anim_hover_in(
				data.clone(),
				state.clone(),
				event_data.widget_id,
			));
			state.borrow_mut().hovered = true;
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
		data.rect_id,
		EventListenerKind::MouseLeave,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common.alterables.animate(anim_hover_out(
				data.clone(),
				state.clone(),
				event_data.widget_id,
			));
			state.borrow_mut().hovered = false;
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
		data.rect_id,
		EventListenerKind::MousePress,
		Box::new(move |common, event_data, _, _| {
			let mut state = state.borrow_mut();

			let rect = event_data.obj.get_as_mut::<WidgetRectangle>();
			anim_hover(rect, &data, 1.0, true);

			if state.hovered {
				state.down = true;
			}

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			Ok(())
		}),
	);
}

fn register_event_mouse_release<U1, U2>(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.rect_id,
		EventListenerKind::MouseRelease,
		Box::new(move |common, event_data, _, _| {
			let rect = event_data.obj.get_as_mut::<WidgetRectangle>();
			anim_hover(rect, &data, 1.0, false);

			let mut state = state.borrow_mut();
			if state.down {
				state.down = false;

				if state.hovered {
					if let Some(on_click) = &state.on_click {
						on_click(ButtonClickEvent {
							state: common.state,
							alterables: common.alterables,
						})?;
					}
				}
			}

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			Ok(())
		}),
	);
}

pub fn construct<U1, U2>(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<Rc<ComponentButton>> {
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.padding = length(1.0);

	let globals = layout.state.globals.clone();

	let (rect_id, _) = layout.add_child(
		parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color: params.color,
			color2: get_color2(&params.color),
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
		WidgetLabel::create(
			&mut globals.i18n(),
			WidgetLabelParams {
				content: params.text,
				style: TextStyle {
					weight: Some(FontWeight::Bold),
					color: Some(if light_text {
						Color::new(1.0, 1.0, 1.0, 1.0)
					} else {
						Color::new(0.0, 0.0, 0.0, 1.0)
					}),
					..params.text_style
				},
			},
		)?,
		taffy::Style {
			..Default::default()
		},
	)?;

	let data = Rc::new(Data {
		text_id,
		rect_id,
		text_node,
		initial_color: params.color,
		initial_color2: get_color2(&params.color),
		initial_border_color: params.border_color,
	});

	let state = Rc::new(RefCell::new(State {
		down: false,
		hovered: false,
		on_click: None,
	}));

	let mut base = ComponentBase::default();

	register_event_mouse_enter(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_leave(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_press(data.clone(), state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_release(data.clone(), state.clone(), listeners, &mut base.lhandles);

	let button = Rc::new(ComponentButton { base, data, state });

	layout.defer_component_init(Component(button.clone()));
	Ok(button)
}
