use std::{cell::RefCell, rc::Rc};

use cosmic_text::Buffer;
use glam::{Mat4, Vec2};
use taffy::TraversePartialTree;

use crate::{
	drawing,
	layout::Widget,
	renderer_vk::text::custom_glyph::CustomGlyph,
	stack::{self, ScissorStack, TransformStack},
	widget::{self},
};

use super::{layout::Layout, widget::DrawState};

pub struct ImageHandle {
	// to be implemented, will contain pixel data (RGB or RGBA) loaded via "ImageBank" or something by the gui
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Boundary {
	pub pos: Vec2,
	pub size: Vec2,
}

impl Boundary {
	pub const fn from_pos_size(pos: Vec2, size: Vec2) -> Self {
		Self { pos, size }
	}

	pub const fn construct(transform_stack: &TransformStack) -> Self {
		let transform = transform_stack.get();

		Self {
			pos: Vec2::new(transform.pos.x, transform.pos.y),
			size: Vec2::new(transform.dim.x, transform.dim.y),
		}
	}

	pub const fn top(&self) -> f32 {
		self.pos.y
	}

	pub const fn bottom(&self) -> f32 {
		self.pos.y + self.size.y
	}

	pub const fn left(&self) -> f32 {
		self.pos.x
	}

	pub const fn right(&self) -> f32 {
		self.pos.x + self.size.x
	}
}

#[derive(Debug, Copy, Clone)]
pub struct Color {
	pub r: f32,
	pub g: f32,
	pub b: f32,
	pub a: f32,
}

impl Color {
	pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
		Self { r, g, b, a }
	}

	#[must_use]
	pub fn add_rgb(&self, n: f32) -> Self {
		Self {
			r: self.r + n,
			g: self.g + n,
			b: self.b + n,
			a: self.a,
		}
	}

	#[must_use]
	pub fn mult_rgb(&self, n: f32) -> Self {
		Self {
			r: self.r * n,
			g: self.g * n,
			b: self.b * n,
			a: self.a,
		}
	}

	#[must_use]
	pub fn lerp(&self, other: &Self, n: f32) -> Self {
		Self {
			r: self.r * (1.0 - n) + other.r * n,
			g: self.g * (1.0 - n) + other.g * n,
			b: self.b * (1.0 - n) + other.b * n,
			a: self.a * (1.0 - n) + other.a * n,
		}
	}
}

impl Default for Color {
	fn default() -> Self {
		// opaque black
		Self::new(0.0, 0.0, 0.0, 1.0)
	}
}

#[repr(u8)]
#[derive(Debug, Default, Clone, Copy)]
pub enum GradientMode {
	#[default]
	None,
	Horizontal,
	Vertical,
	Radial,
}

#[derive(Default, Clone, Copy)]
pub struct Rectangle {
	pub color: Color,
	pub color2: Color,
	pub gradient: GradientMode,

	pub border: f32, // width in pixels
	pub border_color: Color,

	pub round_units: u8,
}

pub struct PrimitiveExtent {
	pub(super) boundary: Boundary,
	pub(super) transform: Mat4,
}

pub enum RenderPrimitive {
	Rectangle(PrimitiveExtent, Rectangle),
	Text(PrimitiveExtent, Rc<RefCell<Buffer>>),
	Sprite(PrimitiveExtent, Option<CustomGlyph>), //option because we want as_slice
	ScissorEnable(Boundary),
	ScissorDisable,
}

pub struct DrawParams<'a> {
	pub layout: &'a Layout,
	pub debug_draw: bool,
}

fn has_overflow_clip(style: &taffy::Style) -> bool {
	style.overflow.x != taffy::Overflow::Visible || style.overflow.y != taffy::Overflow::Visible
}

fn primitive_debug_rect(boundary: &Boundary, transform: &Mat4, color: drawing::Color) -> drawing::RenderPrimitive {
	drawing::RenderPrimitive::Rectangle(
		PrimitiveExtent {
			boundary: *boundary,
			transform: *transform,
		},
		Rectangle {
			border: 1.0,
			border_color: color,
			color: Color::new(0.0, 0.0, 0.0, 0.0),
			..Default::default()
		},
	)
}

fn draw_widget(
	params: &DrawParams,
	state: &mut DrawState,
	node_id: taffy::NodeId,
	style: &taffy::Style,
	widget: &Widget,
	parent_transform: &glam::Mat4,
) {
	let Ok(l) = params.layout.state.tree.layout(node_id) else {
		debug_assert!(false);
		return;
	};

	let mut widget_state = widget.state();

	let transform = widget_state.data.transform * *parent_transform;

	let (shift, info) = match widget::get_scrollbar_info(l) {
		Some(info) => (widget_state.get_scroll_shift(&info, l), Some(info)),
		None => (Vec2::default(), None),
	};

	state.transform_stack.push(stack::Transform {
		pos: Vec2::new(l.location.x, l.location.y) - shift,
		transform,
		dim: Vec2::new(l.size.width, l.size.height),
	});

	if params.debug_draw {
		let boundary = drawing::Boundary::construct(state.transform_stack);
		state.primitives.push(primitive_debug_rect(
			&boundary,
			&transform,
			Color::new(0.0, 1.0, 1.0, 0.5),
		));
	}

	let scissor_pushed = info.is_some() && has_overflow_clip(style);

	if scissor_pushed {
		let boundary = drawing::Boundary::construct(state.transform_stack);
		state.scissor_stack.push(boundary);
		if params.debug_draw {
			state.primitives.push(primitive_debug_rect(
				&boundary,
				&transform,
				Color::new(1.0, 0.0, 1.0, 1.0),
			));
		}

		state.primitives.push(drawing::RenderPrimitive::ScissorEnable(boundary));
	}

	let draw_params = widget::DrawParams {
		node_id,
		taffy_layout: l,
		style,
	};

	widget_state.draw_all(state, &draw_params);

	draw_children(params, state, node_id, &transform);

	if scissor_pushed {
		state.primitives.push(drawing::RenderPrimitive::ScissorDisable);
		state.scissor_stack.pop();
	}

	state.transform_stack.pop();

	if let Some(info) = &info {
		widget_state.draw_scrollbars(state, &draw_params, info);
	}
}

fn draw_children(params: &DrawParams, state: &mut DrawState, parent_node_id: taffy::NodeId, model: &glam::Mat4) {
	let layout = &params.layout;

	for node_id in layout.state.tree.child_ids(parent_node_id) {
		let Some(widget_id) = layout.state.tree.get_node_context(node_id).copied() else {
			debug_assert!(false);
			continue;
		};

		let Ok(style) = layout.state.tree.style(node_id) else {
			debug_assert!(false);
			continue;
		};

		let Some(widget) = layout.state.widgets.get(widget_id) else {
			debug_assert!(false);
			continue;
		};

		draw_widget(params, state, node_id, style, widget, model);
	}
}

pub fn draw(params: &DrawParams) -> anyhow::Result<Vec<RenderPrimitive>> {
	let mut primitives = Vec::<RenderPrimitive>::new();
	let mut transform_stack = TransformStack::new();
	let mut scissor_stack = ScissorStack::new();
	let model = glam::Mat4::IDENTITY;

	let Some(root_widget) = params.layout.state.widgets.get(params.layout.root_widget) else {
		panic!();
	};

	let Ok(style) = params.layout.state.tree.style(params.layout.root_node) else {
		panic!();
	};

	scissor_stack.push(Boundary {
		pos: Default::default(),
		size: Vec2::splat(1.0e12),
	});

	let mut state = DrawState {
		primitives: &mut primitives,
		transform_stack: &mut transform_stack,
		scissor_stack: &mut scissor_stack,
		layout: params.layout,
	};

	draw_widget(params, &mut state, params.layout.root_node, style, root_widget, &model);

	Ok(primitives)
}
