use std::{cell::RefCell, rc::Rc};
use taffy::{
	AlignItems, JustifyContent,
	prelude::{length, percent},
};

use crate::{
	animation::{Animation, AnimationEasing},
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::Color,
	event::{CallbackDataCommon, EventAlterables, EventListenerCollection, EventListenerKind, ListenerHandleVec},
	i18n::Translation,
	layout::{self, Layout, LayoutState, WidgetID, WidgetPair},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

pub struct Params {
	pub text: Translation,
	pub style: taffy::Style,
	pub box_size: f32,
	pub checked: bool,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Translation::from_raw_text(""),
			style: Default::default(),
			box_size: 24.0,
			checked: false,
		}
	}
}

pub struct CheckboxToggleEvent {
	pub checked: bool,
}

pub type CheckboxToggleCallback = Box<dyn Fn(&mut CallbackDataCommon, CheckboxToggleEvent) -> anyhow::Result<()>>;

struct State {
	checked: bool,
	hovered: bool,
	down: bool,
	on_toggle: Option<CheckboxToggleCallback>,
}

#[allow(clippy::struct_field_names)]
struct Data {
	id_container: WidgetID, // Rectangle, transparent if not hovered

	//id_outer_box: WidgetID, // Rectangle, parent of container
	id_inner_box: WidgetID, // Rectangle, parent of outer_box
	id_label: WidgetID,     // Label, parent of container
}

pub struct ComponentCheckbox {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
}

impl ComponentTrait for ComponentCheckbox {
	fn base(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn init(&self, _data: &mut InitData) {}
}

const COLOR_CHECKED: Color = Color::new(0.1, 0.5, 1.0, 1.0);
const COLOR_UNCHECKED: Color = Color::new(0.1, 0.5, 1.0, 0.0);

fn set_box_checked(widgets: &layout::WidgetMap, data: &Data, checked: bool) {
	widgets.call(data.id_inner_box, |rect: &mut WidgetRectangle| {
		rect.params.color = if checked { COLOR_CHECKED } else { COLOR_UNCHECKED }
	});
}

impl ComponentCheckbox {
	pub fn set_text(&self, state: &LayoutState, common: &mut CallbackDataCommon, text: Translation) {
		let Some(mut label) = state.widgets.get_as::<WidgetLabel>(self.data.id_label) else {
			return;
		};

		label.set_text(common, text);
	}

	pub fn set_checked(&self, state: &LayoutState, alterables: &mut EventAlterables, checked: bool) {
		self.state.borrow_mut().checked = checked;
		set_box_checked(&state.widgets, &self.data, checked);
		alterables.mark_redraw();
	}

	pub fn on_toggle(&self, func: CheckboxToggleCallback) {
		self.state.borrow_mut().on_toggle = Some(func);
	}
}

fn anim_hover(rect: &mut WidgetRectangle, pos: f32, pressed: bool) {
	let brightness = pos * if pressed { 0.6 } else { 0.4 };
	rect.params.border = 2.0;
	rect.params.color.a = brightness;
	rect.params.border_color.a = rect.params.color.a;
	if pressed {
		rect.params.border_color.a += 0.4;
	}
}

fn anim_hover_in(state: Rc<RefCell<State>>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		5,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(rect, anim_data.pos, state.borrow().down);
			common.alterables.mark_redraw();
		}),
	)
}

fn anim_hover_out(state: Rc<RefCell<State>>, widget_id: WidgetID) -> Animation {
	Animation::new(
		widget_id,
		8,
		AnimationEasing::OutQuad,
		Box::new(move |common, anim_data| {
			let rect = anim_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(rect, 1.0 - anim_data.pos, state.borrow().down);
			common.alterables.mark_redraw();
		}),
	)
}

fn register_event_mouse_enter<U1, U2>(
	data: &Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.id_container,
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common
				.alterables
				.animate(anim_hover_in(state.clone(), event_data.widget_id));
			state.borrow_mut().hovered = true;
			Ok(())
		}),
	);
}

fn register_event_mouse_leave<U1, U2>(
	data: &Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.id_container,
		EventListenerKind::MouseLeave,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			common
				.alterables
				.animate(anim_hover_out(state.clone(), event_data.widget_id));
			state.borrow_mut().hovered = false;
			Ok(())
		}),
	);
}

fn register_event_mouse_press<U1, U2>(
	data: &Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection<U1, U2>,
	listener_handles: &mut ListenerHandleVec,
) {
	listeners.register(
		listener_handles,
		data.id_container,
		EventListenerKind::MousePress,
		Box::new(move |common, event_data, _, _| {
			let mut state = state.borrow_mut();

			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(rect, 1.0, true);

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
		data.id_container,
		EventListenerKind::MouseRelease,
		Box::new(move |common, event_data, _, _| {
			let rect = event_data.obj.get_as_mut::<WidgetRectangle>().unwrap();
			anim_hover(rect, 1.0, false);

			let mut state = state.borrow_mut();
			if state.down {
				state.down = false;

				state.checked = !state.checked;
				set_box_checked(&common.state.widgets, &data, state.checked);

				if state.hovered
					&& let Some(on_toggle) = &state.on_toggle
				{
					on_toggle(common, CheckboxToggleEvent { checked: state.checked })?;
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
) -> anyhow::Result<(WidgetPair, Rc<ComponentCheckbox>)> {
	let mut style = params.style;

	// force-override style
	style.flex_wrap = taffy::FlexWrap::NoWrap;
	style.align_items = Some(AlignItems::Center);
	style.justify_content = Some(JustifyContent::Center);
	style.padding = taffy::Rect {
		left: length(4.0),
		right: length(8.0),
		top: length(4.0),
		bottom: length(4.0),
	};
	//style.align_self = Some(taffy::AlignSelf::Start); // do not stretch self to the parent
	style.gap = length(4.0);

	let globals = layout.state.globals.clone();

	let (root, _) = layout.add_child(
		parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color: Color::new(1.0, 1.0, 1.0, 0.0),
			border_color: Color::new(1.0, 1.0, 1.0, 0.0),
			round: WLength::Units(5.0),
			..Default::default()
		}),
		style,
	)?;

	let id_container = root.id;

	let box_size = taffy::Size {
		width: length(params.box_size),
		height: length(params.box_size),
	};

	let (outer_box, _) = layout.add_child(
		id_container,
		WidgetRectangle::create(WidgetRectangleParams {
			border: 2.0,
			border_color: Color::new(1.0, 1.0, 1.0, 1.0),
			round: WLength::Units(8.0),
			color: Color::new(1.0, 1.0, 1.0, 0.0),
			..Default::default()
		}),
		taffy::Style {
			size: box_size,
			padding: taffy::Rect::length(4.0),
			min_size: box_size,
			max_size: box_size,
			..Default::default()
		},
	)?;

	let (inner_box, _) = layout.add_child(
		outer_box.id,
		WidgetRectangle::create(WidgetRectangleParams {
			round: WLength::Units(5.0),
			color: if params.checked { COLOR_CHECKED } else { COLOR_UNCHECKED },
			..Default::default()
		}),
		taffy::Style {
			size: taffy::Size {
				width: percent(1.0),
				height: percent(1.0),
			},
			..Default::default()
		},
	)?;

	let (label, _node_label) = layout.add_child(
		id_container,
		WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: params.text,
				style: TextStyle {
					weight: Some(FontWeight::Bold),
					..Default::default()
				},
			},
		),
		Default::default(),
	)?;

	let data = Rc::new(Data {
		id_container,
		id_inner_box: inner_box.id,
		id_label: label.id,
	});

	let state = Rc::new(RefCell::new(State {
		checked: params.checked,
		down: false,
		hovered: false,
		on_toggle: None,
	}));

	let mut base = ComponentBase::default();

	register_event_mouse_enter(&data, state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_leave(&data, state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_press(&data, state.clone(), listeners, &mut base.lhandles);
	register_event_mouse_release(data.clone(), state.clone(), listeners, &mut base.lhandles);

	let checkbox = Rc::new(ComponentCheckbox { base, data, state });

	layout.defer_component_init(Component(checkbox.clone()));
	Ok((root, checkbox))
}
