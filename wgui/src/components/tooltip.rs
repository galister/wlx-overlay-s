use std::{cell::RefCell, rc::Rc};
use taffy::prelude::length;

use crate::{
	components::{Component, ComponentBase, ComponentTrait, InitData},
	drawing::Color,
	event::{EventListenerCollection, EventListenerKind},
	i18n::Translation,
	layout::{LayoutTasks, WidgetID, WidgetPair},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
		ConstructEssentials, EventResult,
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

fn register_event_mouse_enter(listeners: &mut EventListenerCollection) -> crate::event::EventListenerID {
	listeners.register(
		EventListenerKind::MouseEnter,
		Box::new(move |common, _event_data, (), ()| {
			common.alterables.trigger_haptics();
			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_leave(listeners: &mut EventListenerCollection) -> crate::event::EventListenerID {
	listeners.register(
		EventListenerKind::MouseEnter,
		Box::new(move |common, _event_data, (), ()| {
			common.alterables.trigger_haptics();
			Ok(EventResult::Pass)
		}),
	)
}

pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentTooltip>)> {
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

	let base = ComponentBase {
		lhandles: {
			let mut widget = ess.layout.state.widgets.get(id_container).unwrap().state();
			vec![
				register_event_mouse_enter(&mut widget.event_listeners),
				register_event_mouse_leave(&mut widget.event_listeners),
			]
		},
	};

	let tooltip = Rc::new(ComponentTooltip {
		base,
		data,
		state,
		tasks: ess.layout.tasks.clone(),
	});

	ess.layout.defer_component_init(Component(tooltip.clone()));
	Ok((root, tooltip))
}
