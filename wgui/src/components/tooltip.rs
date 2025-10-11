use std::{cell::RefCell, rc::Rc};
use taffy::prelude::length;

use crate::{
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::Color,
	event::{EventListenerCollection, EventListenerKind, ListenerHandleVec},
	i18n::Translation,
	layout::{LayoutTasks, WidgetID, WidgetPair},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		ConstructEssentials, EventResult,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

pub struct Params {
	pub text: Translation,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Translation::from_raw_text(""),
		}
	}
}

struct State {}

#[allow(clippy::struct_field_names)]
struct Data {
	id_container: WidgetID, // Rectangle
	id_label: WidgetID,     // Label, parent of container
}

pub struct ComponentTooltip {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	tasks: LayoutTasks,
}

impl ComponentTrait for ComponentTooltip {
	fn base(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn init(&self, _data: &mut InitData) {}
}

impl ComponentTooltip {}

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
			Ok(EventResult::Pass)
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
		EventListenerKind::MouseEnter,
		Box::new(move |common, event_data, _, _| {
			common.alterables.trigger_haptics();
			Ok(EventResult::Pass)
		}),
	);
}

pub fn construct<U1, U2>(
	ess: &mut ConstructEssentials<U1, U2>,
	params: Params,
) -> anyhow::Result<(WidgetPair, Rc<ComponentTooltip>)> {
	let style = taffy::Style {
		align_items: Some(taffy::AlignItems::Center),
		justify_content: Some(taffy::JustifyContent::Center),
		gap: length(4.0),
		padding: taffy::Rect {
			left: length(8.0),
			right: length(8.0),
			top: length(4.0),
			bottom: length(4.0),
		},
		..Default::default()
	};

	let globals = ess.layout.state.globals.clone();

	let (root, _) = ess.layout.add_child(
		ess.parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color: Color::new(1.0, 1.0, 1.0, 0.0),
			border_color: Color::new(1.0, 1.0, 1.0, 0.0),
			round: WLength::Units(5.0),
			..Default::default()
		}),
		style,
	)?;

	let id_container = root.id;

	let (label, _node_label) = ess.layout.add_child(
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
		id_label: label.id,
	});

	let state = Rc::new(RefCell::new(State {}));

	let mut base = ComponentBase::default();

	register_event_mouse_enter(&data, state.clone(), ess.listeners, &mut base.lhandles);
	register_event_mouse_leave(&data, state.clone(), ess.listeners, &mut base.lhandles);

	let tooltip = Rc::new(ComponentTooltip {
		base,
		data,
		state,
		tasks: ess.layout.tasks.clone(),
	});

	ess.layout.defer_component_init(Component(tooltip.clone()));
	Ok((root, tooltip))
}
