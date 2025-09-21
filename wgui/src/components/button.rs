use std::{cell::RefCell, rc::Rc};
use taffy::{AlignItems, JustifyContent, prelude::length};

use crate::{
	animation::{Animation, AnimationEasing},
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::{self, Color},
	event::{CallbackDataCommon, EventListenerCollection, EventListenerKind, ListenerHandleVec},
	globals::Globals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

pub struct Params {
	pub text: Translation,
	pub color: Option<drawing::Color>,
	pub border_color: Option<drawing::Color>,
	pub hover_border_color: Option<drawing::Color>,
	pub hover_color: Option<drawing::Color>,
	pub round: WLength,
	pub style: taffy::Style,
	pub text_style: TextStyle,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Translation::from_raw_text(""),
			color: None,
			hover_color: None,
			border_color: None,
			hover_border_color: None,
			round: WLength::Units(4.0),
			style: Default::default(),
			text_style: TextStyle::default(),
		}
	}
}

pub struct ButtonClickEvent {}
pub type ButtonClickCallback = Box<dyn Fn(&mut CallbackDataCommon, ButtonClickEvent) -> anyhow::Result<()>>;

struct State {
	hovered: bool,
	down: bool,
	on_click: Option<ButtonClickCallback>,
}

struct Data {
	initial_color: drawing::Color,
	initial_border_color: drawing::Color,
	initial_hover_color: drawing::Color,
	initial_hover_border_color: drawing::Color,
	id_label: WidgetID, // Label
	id_rect: WidgetID,  // Rectangle
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
	pub fn set_text(&self, common: &mut CallbackDataCommon, text: Translation) {
		let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(self.data.id_label) else {
			return;
		};

		label.set_text(common, text);
	}

	pub fn on_click(&self, func: ButtonClickCallback) {
		self.state.borrow_mut().on_click = Some(func);
	}
}

fn get_color2(color: &drawing::Color) -> drawing::Color {
	color.lerp(&Color::new(0.0, 0.0, 0.0, color.a), 0.2)
}

fn anim_hover(rect: &mut WidgetRectangle, data: &Data, pos: f32, pressed: bool) {
	let mult = pos * if pressed { 1.5 } else { 1.0 };
	let bgcolor = data.initial_color.lerp(&data.initial_hover_color, mult);

	rect.params.color = bgcolor;
	rect.params.color2 = get_color2(&bgcolor);
	rect.params.border_color = data.initial_border_color.lerp(&data.initial_hover_border_color, mult);
	rect.params.border = 2.0;
}

fn anim_hover_out(data: Rc<Data>, state: Rc<RefCell<State>>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		15,
		AnimationEasing::OutCubic,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
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
		data.id_rect,
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();
			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			let mut state = state.borrow_mut();
			anim_hover(rect, &data, 1.0, state.down);
			state.hovered = true;
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
		data.id_rect,
		EventListenerKind::MouseLeave,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common
				.alterables
				.animate(anim_hover_out(data.clone(), state.clone(), event_data.widget_id));
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
		data.id_rect,
		EventListenerKind::MousePress,
		Box::new(move |common, event_data, _, _| {
			let mut state = state.borrow_mut();

			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
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
		data.id_rect,
		EventListenerKind::MouseRelease,
		Box::new(move |common, event_data, _, _| {
			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(rect, &data, 1.0, false);

			let mut state = state.borrow_mut();
			if state.down {
				state.down = false;

				if state.hovered
					&& let Some(on_click) = &state.on_click
				{
					on_click(common, ButtonClickEvent {})?;
				}
			}

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			Ok(())
		}),
	);
}

pub fn construct<U1, U2>(
	globals: &mut Globals,
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent: WidgetID,
	params: Params,
) -> anyhow::Result<(WidgetID, Rc<ComponentButton>)> {
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.padding = length(1.0);

	// update colors to default ones if they are not specified
	let color = if let Some(color) = params.color {
		color
	} else {
		globals.defaults.button_color
	};

	let border_color = if let Some(border_color) = params.border_color {
		border_color
	} else {
		Color::new(color.r, color.g, color.b, color.a + 0.4)
	};

	let hover_color = if let Some(hover_color) = params.hover_color {
		hover_color
	} else {
		Color::new(color.r + 0.25, color.g + 0.25, color.g + 0.25, color.a + 0.25)
	};

	let hover_border_color = if let Some(hover_border_color) = params.hover_border_color {
		hover_border_color
	} else {
		Color::new(color.r + 0.5, color.g + 0.5, color.g + 0.5, color.a + 0.5)
	};

	let (id_root, _) = layout.add_child(
		parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color,
			color2: get_color2(&color),
			gradient: drawing::GradientMode::Vertical,
			round: params.round,
			border_color,
			border: 2.0,
		}),
		style,
	)?;
	let id_rect = id_root;

	let light_text = (color.r + color.g + color.b) < 1.5;

	let (id_label, _node_label) = layout.add_child(
		id_rect,
		WidgetLabel::create(
			globals,
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
		),
		Default::default(),
	)?;

	let data = Rc::new(Data {
		id_label,
		id_rect,
		initial_color: color,
		initial_border_color: border_color,
		initial_hover_color: hover_color,
		initial_hover_border_color: hover_border_color,
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
	Ok((id_root, button))
}
