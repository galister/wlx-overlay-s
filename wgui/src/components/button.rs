use crate::{
	animation::{Animation, AnimationEasing},
	components::{self, Component, ComponentBase, ComponentTrait, RefreshData, tooltip::ComponentTooltip},
	drawing::{self, Boundary, Color},
	event::{CallbackDataCommon, EventListenerCollection, EventListenerID, EventListenerKind},
	i18n::Translation,
	layout::{LayoutTask, WidgetID, WidgetPair},
	renderer_vk::{
		text::{FontWeight, TextStyle},
		util::centered_matrix,
	},
	widget::{
		self, ConstructEssentials, EventResult, WidgetData,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};
use glam::{Mat4, Vec3};
use std::{cell::RefCell, rc::Rc};
use taffy::{AlignItems, JustifyContent, prelude::length};

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
	/// make this a toggle-style button that stays depressed
	/// until "un-clicked". this is visual only.
	/// set the initial state using `set_sticky_state`
	pub sticky: bool,
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
			sticky: false,
		}
	}
}

pub struct ButtonClickEvent {}
pub type ButtonClickCallback = Box<dyn Fn(&mut CallbackDataCommon, ButtonClickEvent) -> anyhow::Result<()>>;

struct State {
	hovered: bool,
	down: bool,
	sticky_down: bool,
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
	sticky: bool,
}

pub struct ComponentButton {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
}

impl ComponentTrait for ComponentButton {
	fn base(&self) -> &ComponentBase {
		&self.base
	}

	fn base_mut(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn refresh(&self, data: &mut RefreshData) {
		// nothing to do
		let mut state = self.state.borrow_mut();

		if state.active_tooltip.is_some() {
			if let Some(node_id) = data.common.state.nodes.get(self.base.get_id()) {
				if !widget::is_node_visible(&data.common.state.tree, *node_id) {
					state.active_tooltip = None; // destroy the tooltip, this button is now hidden
				}
			} else {
				debug_assert!(false);
			}
		}
	}
}

impl ComponentButton {
	pub fn get_label(&self) -> WidgetID {
		self.data.id_label
	}

	pub fn get_rect(&self) -> WidgetID {
		self.data.id_rect
	}

	pub fn set_text(&self, common: &mut CallbackDataCommon, text: Translation) {
		let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(self.data.id_label) else {
			return;
		};

		label.set_text(common, text);
	}

	pub fn on_click(&self, func: ButtonClickCallback) {
		self.state.borrow_mut().on_click = Some(func);
	}

	/// Sets the sticky state of the button.
	///
	/// On buttons where sticky is false, sticky state won't automatically clear.
	pub fn set_sticky_state(&self, common: &mut CallbackDataCommon, sticky_down: bool) {
		let mut state = self.state.borrow_mut();

		// only play anim if we're not changing the border highlight
		let dirty = !state.hovered && !state.down && state.sticky_down != sticky_down;

		state.sticky_down = sticky_down;

		if !dirty {
			return;
		}

		let data = self.data.clone();
		let anim = Animation::new(
			self.data.id_rect,
			if sticky_down { 5 } else { 10 },
			AnimationEasing::OutCubic,
			Box::new(move |common, anim_data| {
				let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
				let mult = if sticky_down {
					anim_data.pos
				} else {
					1.0 - anim_data.pos
				};

				let bgcolor = data.initial_color.lerp(&data.initial_hover_color, mult * 0.5);
				rect.params.color = bgcolor;
				rect.params.color2 = get_color2(&bgcolor);
				rect.params.border_color = data.initial_border_color.lerp(&data.initial_hover_border_color, mult);
				common.alterables.mark_redraw();
			}),
		);

		common.alterables.animations.push(anim);
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
	sticky_down: bool,
) {
	let mult = pos * if pressed { 1.5 } else { 1.0 };

	let (init_border_color, init_color) = if sticky_down {
		(
			data.initial_hover_border_color,
			data.initial_color.lerp(&data.initial_hover_color, 0.5),
		)
	} else {
		(data.initial_border_color, data.initial_color)
	};

	let bgcolor = init_color.lerp(&data.initial_hover_color, mult);

	//let t = Mat4::from_scale(Vec3::splat(1.0 + pos * 0.5)) * Mat4::from_rotation_z(pos * 1.0);

	let t = Mat4::from_scale(Vec3::splat(1.0 + pos * 0.05));
	widget_data.transform = centered_matrix(widget_boundary.size, &t);

	rect.params.color = bgcolor;
	rect.params.color2 = get_color2(&bgcolor);

	rect.params.border_color = init_border_color.lerp(&data.initial_hover_border_color, mult);
}

fn anim_hover_create(data: Rc<Data>, state: Rc<RefCell<State>>, widget_id: WidgetID, fade_in: bool) -> Animation {
	Animation::new(
		widget_id,
		if fade_in { 5 } else { 10 },
		AnimationEasing::OutCubic,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			let state = state.borrow();
			anim_hover(
				rect,
				anim_data.data,
				&data,
				anim_data.widget_boundary,
				if fade_in { anim_data.pos } else { 1.0 - anim_data.pos },
				state.down,
				state.sticky_down,
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
				state.sticky_down,
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
			let mut state = state.borrow_mut();

			if data.sticky {
				state.sticky_down = !state.sticky_down;
			}

			anim_hover(
				rect,
				event_data.widget_data,
				&data,
				common.state.get_node_boundary(event_data.node_id),
				1.0,
				false,
				state.sticky_down,
			);

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

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

#[allow(clippy::too_many_lines)]
pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentButton>)> {
	let globals = ess.layout.state.globals.clone();
	let mut style = params.style;

	// force-override style
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.overflow.x = taffy::Overflow::Hidden;
	style.overflow.y = taffy::Overflow::Hidden;
	style.gap = length(4.0);

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

	let light_text = {
		let mult = if globals.get().defaults.dark_mode {
			color.a
		} else {
			1.0 - color.a
		};
		(color.r + color.g + color.b) * mult < 1.5
	};

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
		sticky: params.sticky,
	});

	let state = Rc::new(RefCell::new(State {
		down: false,
		hovered: false,
		on_click: None,
		active_tooltip: None,
		sticky_down: false,
	}));

	let base = ComponentBase {
		id: root.id,
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

	ess.layout.register_component_refresh(Component(button.clone()));
	Ok((root, button))
}
