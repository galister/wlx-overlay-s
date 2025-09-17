use std::{cell::RefCell, rc::Rc};

use cosmic_text::Buffer;
use glam::{Mat4, Vec2};
use taffy::TraversePartialTree;

use crate::{
	layout::Widget,
	renderer_vk::text::custom_glyph::CustomGlyph,
	transform_stack::{self, TransformStack},
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

pub struct RenderPrimitive {
	pub(super) boundary: Boundary,
	pub(super) transform: Mat4,
	pub(super) depth: f32,
	pub(super) payload: PrimitivePayload,
}

pub enum PrimitivePayload {
	Rectangle(Rectangle),
	Text(Rc<RefCell<Buffer>>),
	Sprite(Option<CustomGlyph>), //option because we want as_slice
}

fn draw_widget(
	layout: &Layout,
	state: &mut DrawState,
	node_id: taffy::NodeId,
	style: &taffy::Style,
	widget: &Widget,
	parent_transform: &glam::Mat4,
) {
	let Ok(l) = layout.state.tree.layout(node_id) else {
		debug_assert!(false);
		return;
	};

	let mut widget_state = widget.state();

	let transform = widget_state.data.transform * *parent_transform;

	let (shift, info) = match widget::get_scrollbar_info(l) {
		Some(info) => (widget_state.get_scroll_shift(&info, l), Some(info)),
		None => (Vec2::default(), None),
	};

	state.transform_stack.push(transform_stack::Transform {
		pos: Vec2::new(l.location.x, l.location.y) - shift,
		transform,
		dim: Vec2::new(l.size.width, l.size.height),
	});

	let draw_params = widget::DrawParams {
		node_id,
		taffy_layout: l,
		style,
	};

	widget_state.draw_all(state, &draw_params);

	draw_children(layout, state, node_id, &transform);

	state.transform_stack.pop();

	if let Some(info) = &info {
		widget_state.draw_scrollbars(state, &draw_params, info);
	}
}

fn draw_children(layout: &Layout, state: &mut DrawState, parent_node_id: taffy::NodeId, model: &glam::Mat4) {
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

		state.depth += 0.01;
		draw_widget(layout, state, node_id, style, widget, model);
		state.depth -= 0.01;
	}
}

pub fn draw(layout: &Layout) -> anyhow::Result<Vec<RenderPrimitive>> {
	let mut primitives = Vec::<RenderPrimitive>::new();
	let mut transform_stack = TransformStack::new();
	let model = glam::Mat4::IDENTITY;

	let Some(root_widget) = layout.state.widgets.get(layout.root_widget) else {
		panic!();
	};

	let Ok(style) = layout.state.tree.style(layout.root_node) else {
		panic!();
	};

	let mut params = DrawState {
		primitives: &mut primitives,
		transform_stack: &mut transform_stack,
		layout,
		depth: 0.0,
	};

	draw_widget(layout, &mut params, layout.root_node, style, root_widget, &model);

	Ok(primitives)
}
