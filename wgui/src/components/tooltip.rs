use glam::{Mat4, Vec3};
use std::{cell::RefCell, rc::Rc};
use taffy::prelude::length;

use crate::{
	components::{self, Component, ComponentBase, ComponentTrait, InitData},
	drawing::Color,
	i18n::Translation,
	layout::{self, LayoutTask, LayoutTasks, WidgetID, WidgetPair},
	renderer_vk::text::{FontWeight, TextStyle},
	widget::{
		ConstructEssentials,
		div::WidgetDiv,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		util::WLength,
	},
};

#[derive(Clone, Default)]
pub enum TooltipSide {
	Left,
	Right,
	Top,
	#[default]
	Bottom,
}

#[derive(Clone)]
pub struct TooltipInfo {
	pub text: Translation,
	pub side: TooltipSide,
}

pub struct Params {
	pub info: TooltipInfo,
	pub widget_to_watch: WidgetID,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			info: TooltipInfo {
				text: Translation::from_raw_text(""),
				side: TooltipSide::Bottom,
			},
			widget_to_watch: WidgetID::default(),
		}
	}
}

struct State {}

#[allow(clippy::struct_field_names)]
struct Data {
	id_root: WidgetID, // Rectangle
}

pub struct ComponentTooltip {
	base: ComponentBase,
	data: Rc<Data>,
	#[allow(dead_code)]
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

impl Drop for ComponentTooltip {
	fn drop(&mut self) {
		self.tasks.push(LayoutTask::RemoveWidget(self.data.id_root));
	}
}

#[allow(clippy::too_many_lines)]
pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentTooltip>)> {
	let absolute_boundary = {
		let widget_to_watch = ess
			.layout
			.state
			.widgets
			.get(params.widget_to_watch)
			.ok_or_else(|| anyhow::anyhow!("widget_to_watch is invalid"))?;
		let widget_to_watch_state = widget_to_watch.state();
		widget_to_watch_state.data.cached_absolute_boundary
	};

	let spacing = 8.0;

	let transform = Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.0));

	let (pin_left, pin_top, pin_align_items, pin_justify_content) = match params.info.side {
		TooltipSide::Left => (
			absolute_boundary.left() - spacing,
			absolute_boundary.top() + absolute_boundary.size.y / 2.0,
			taffy::AlignItems::Center,
			taffy::JustifyContent::End,
		),
		TooltipSide::Right => (
			absolute_boundary.left() + absolute_boundary.size.x + spacing,
			absolute_boundary.top() + absolute_boundary.size.y / 2.0,
			taffy::AlignItems::Center,
			taffy::JustifyContent::Start,
		),
		TooltipSide::Top => (
			absolute_boundary.left() + absolute_boundary.size.x / 2.0,
			absolute_boundary.top() - spacing,
			taffy::AlignItems::End,
			taffy::JustifyContent::Center,
		),
		TooltipSide::Bottom => (
			absolute_boundary.left() + absolute_boundary.size.x / 2.0,
			absolute_boundary.top() + absolute_boundary.size.y + spacing,
			taffy::AlignItems::Baseline,
			taffy::JustifyContent::Center,
		),
	};

	let globals = ess.layout.state.globals.clone();

	let (div, _) = ess.layout.add_child(
		ess.parent,
		WidgetDiv::create(),
		taffy::Style {
			align_items: Some(pin_align_items),
			justify_content: Some(pin_justify_content),
			position: taffy::Position::Absolute,
			margin: taffy::Rect {
				left: length(pin_left),
				top: length(pin_top),
				bottom: length(0.0),
				right: length(0.0),
			},
			/* important, to make it centered! */
			size: taffy::Size {
				width: length(0.0),
				height: length(0.0),
			},
			..Default::default()
		},
	)?;

	div.widget.state().data.transform = transform;

	let (rect, _) = ess.layout.add_child(
		div.id,
		WidgetRectangle::create(WidgetRectangleParams {
			color: Color::new(0.1, 0.1, 0.1, 0.8),
			border_color: Color::new(0.3, 0.3, 0.3, 1.0),
			border: 2.0,
			round: WLength::Percent(1.0),
			..Default::default()
		}),
		taffy::Style {
			position: taffy::Position::Relative,
			gap: length(4.0),
			padding: taffy::Rect {
				left: length(16.0),
				right: length(16.0),
				top: length(8.0),
				bottom: length(8.0),
			},
			..Default::default()
		},
	)?;

	let (_label, _) = ess.layout.add_child(
		rect.id,
		WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: params.info.text,
				style: TextStyle {
					weight: Some(FontWeight::Bold),
					..Default::default()
				},
			},
		),
		Default::default(),
	)?;

	let data = Rc::new(Data { id_root: div.id });

	let state = Rc::new(RefCell::new(State {}));

	let base = ComponentBase::default();

	let tooltip = Rc::new(ComponentTooltip {
		base,
		data,
		state,
		tasks: ess.layout.tasks.clone(),
	});

	ess.layout.defer_component_init(Component(tooltip.clone()));
	Ok((div, tooltip))
}

pub fn show(
	layout: &mut layout::Layout,
	widget_to_watch: WidgetID,
	info: TooltipInfo,
) -> anyhow::Result<Rc<ComponentTooltip>> {
	let parent = layout.tree_root_widget;
	let (_, tooltip) = components::tooltip::construct(
		&mut ConstructEssentials { layout, parent },
		components::tooltip::Params { info, widget_to_watch },
	)?;
	Ok(tooltip)
}
