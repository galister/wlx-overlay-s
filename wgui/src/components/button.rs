use crate::{
	animation::{Animation, AnimationEasing},
	components::{self, tooltip::ComponentTooltip, Component, ComponentBase, ComponentTrait, InitData},
	drawing::{self, Boundary, Color},
	event::{CallbackDataCommon, EventListenerCollection, EventListenerID, EventListenerKind},
	i18n::Translation,
	layout::{LayoutTask, WidgetID, WidgetPair},
	renderer_vk::{
		text::{FontWeight, TextStyle},
		util::centered_matrix,
	},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
		ConstructEssentials, EventResult, WidgetData,
	},
};
use glam::{Mat4, Vec3};
use std::{cell::RefCell, rc::Rc};
use taffy::{AlignItems, JustifyContent};

pub struct Params {
	pub text: Option<Translation>, // if unset, label will not be populated
	pub color: Option<drawing::Color>,
	pub border: f32,
	pub border_color: Option<drawing::Color>,
	pub hover_border_color: Option<drawing::Color>,
	pub hover_color: Option<drawing::Color>,
	pub round: WLength,
	pub style: taffy::Style,
	pub text_style: TextStyle,
	pub tooltip: Option<components::tooltip::TooltipInfo>,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Some(Translation::from_raw_text("")),
			color: None,
			hover_color: None,
			border_color: None,
			border: 2.0,
			hover_border_color: None,
			round: WLength::Units(4.0),
			style: Default::default(),
			text_style: TextStyle::default(),
			tooltip: None,
		}
	}
}

pub struct ButtonClickEvent {}
pub type ButtonClickCallback = Box<dyn Fn(&mut CallbackDataCommon, ButtonClickEvent) -> anyhow::Result<()>>;

struct State {
	hovered: bool,
	down: bool,
	on_click: Option<ButtonClickCallback>,
	active_tooltip: Option<Rc<ComponentTooltip>>,
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

fn anim_hover(
	rect: &mut WidgetRectangle,
	widget_data: &mut WidgetData,
	data: &Data,
	widget_boundary: Boundary,
	pos: f32,
	pressed: bool,
) {
	let mult = pos * if pressed { 1.5 } else { 1.0 };
	let bgcolor = data.initial_color.lerp(&data.initial_hover_color, mult);

	//let t = Mat4::from_scale(Vec3::splat(1.0 + pos * 0.5)) * Mat4::from_rotation_z(pos * 1.0);

	let t = Mat4::from_scale(Vec3::splat(1.0 + pos * 0.05));
	widget_data.transform = centered_matrix(widget_boundary.size, &t);

	rect.params.color = bgcolor;
	rect.params.color2 = get_color2(&bgcolor);
	rect.params.border_color = data.initial_border_color.lerp(&data.initial_hover_border_color, mult);
}

fn anim_hover_create(data: Rc<Data>, state: Rc<RefCell<State>>, widget_id: WidgetID, fade_in: bool) -> Animation {
	Animation::new(
		widget_id,
		if fade_in { 5 } else { 10 },
		AnimationEasing::OutCubic,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(
				rect,
				anim_data.data,
				&data,
				anim_data.widget_boundary,
				if fade_in { anim_data.pos } else { 1.0 - anim_data.pos },
				state.borrow().down,
			);
			common.alterables.mark_redraw();
		}),
	)
}

fn register_event_mouse_enter(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection,
	info: Option<components::tooltip::TooltipInfo>,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, (), ()| {
			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();
			common.alterables.animate(anim_hover_create(
				data.clone(),
				state.clone(),
				event_data.widget_id,
				true,
			));

			if let Some(info) = info.clone() {
				common.alterables.tasks.push(LayoutTask::ModifyLayoutState({
					let widget_to_watch = data.id_rect;
					let state = state.clone();
					Box::new(move |m| {
						state.borrow_mut().active_tooltip =
							Some(components::tooltip::show(m.layout, widget_to_watch, info.clone())?);
						Ok(())
					})
				}));
			}

			state.borrow_mut().hovered = true;
			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_leave(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseLeave,
		Box::new(move |common, event_data, (), ()| {
			common.alterables.trigger_haptics();
			common.alterables.animate(anim_hover_create(
				data.clone(),
				state.clone(),
				event_data.widget_id,
				false,
			));
			let mut state = state.borrow_mut();
			state.active_tooltip = None;
			state.hovered = false;
			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_press(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MousePress,
		Box::new(move |common, event_data, (), ()| {
			let mut state = state.borrow_mut();

			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(
				rect,
				event_data.widget_data,
				&data,
				common.state.get_node_boundary(event_data.node_id),
				1.0,
				true,
			);

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			if state.hovered {
				state.down = true;
				Ok(EventResult::Consumed)
			} else {
				Ok(EventResult::Pass)
			}
		}),
	)
}

fn register_event_mouse_release(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseRelease,
		Box::new(move |common, event_data, (), ()| {
			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(
				rect,
				event_data.widget_data,
				&data,
				common.state.get_node_boundary(event_data.node_id),
				1.0,
				false,
			);

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			let mut state = state.borrow_mut();
			if state.down {
				state.down = false;

				if state.hovered
					&& let Some(on_click) = &state.on_click
				{
					on_click(common, ButtonClickEvent {})?;
				}
				Ok(EventResult::Consumed)
			} else {
				Ok(EventResult::Pass)
			}
		}),
	)
}

pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>)> {
	let globals = ess.layout.state.globals.clone();
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.overflow.x = taffy::Overflow::Hidden;
	style.overflow.y = taffy::Overflow::Hidden;

	// update colors to default ones if they are not specified
	let color = if let Some(color) = params.color {
		color
	} else {
		globals.get().defaults.button_color
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

	let (root, _) = ess.layout.add_child(
		ess.parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color,
			color2: get_color2(&color),
			gradient: drawing::GradientMode::Vertical,
			round: params.round,
			border_color,
			border: params.border,
		}),
		style,
	)?;

	let id_rect = root.id;

	let light_text = (color.r + color.g + color.b) < 1.5;

	let id_label = if let Some(content) = params.text {
		let (label, _node_label) = ess.layout.add_child(
			id_rect,
			WidgetLabel::create(
				&mut globals.get(),
				WidgetLabelParams {
					content,
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
		label.id
	} else {
		WidgetID::default()
	};

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
		active_tooltip: None,
	}));

	let base = ComponentBase {
		lhandles: {
			let mut widget = ess.layout.state.widgets.get(id_rect).unwrap().state();
			vec![
				register_event_mouse_enter(data.clone(), state.clone(), &mut widget.event_listeners, params.tooltip),
				register_event_mouse_leave(data.clone(), state.clone(), &mut widget.event_listeners),
				register_event_mouse_press(data.clone(), state.clone(), &mut widget.event_listeners),
				register_event_mouse_release(data.clone(), state.clone(), &mut widget.event_listeners),
			]
		},
	};

	let button = Rc::new(ComponentButton { base, data, state });

	ess.layout.defer_component_init(Component(button.clone()));
	Ok((root, button))
}
