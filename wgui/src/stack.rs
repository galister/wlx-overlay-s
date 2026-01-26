use glam::{Mat4, Vec2, Vec3};

use crate::drawing;

pub trait Pushable<T> {
	fn push(&mut self, item: &T);
}

#[derive(Debug)]
pub struct GenericStack<T, const STACK_MAX: usize> {
	pub stack: [T; STACK_MAX],
	top: u8,
}

pub trait StackItem<T>: Default + Clone + Copy + Pushable<T> {}

impl<T: StackItem<T>, const STACK_MAX: usize> GenericStack<T, STACK_MAX> {
	pub fn new() -> Self {
		Self {
			stack: [Default::default(); STACK_MAX],
			top: 1,
		}
	}

	pub fn push(&mut self, mut item: T) {
		assert!(self.top < STACK_MAX as u8);
		let idx = (self.top - 1) as usize;
		let upper_item = &self.stack[idx];
		item.push(upper_item);
		self.stack[self.top as usize] = item;
		self.top += 1;
	}

	pub fn pop(&mut self) {
		assert!(self.top > 0);
		self.top -= 1;
	}

	pub const fn get(&self) -> &T {
		&self.stack[(self.top - 1) as usize]
	}
}

impl<T: StackItem<T>, const STACK_MAX: usize> Default for GenericStack<T, STACK_MAX> {
	fn default() -> Self {
		Self::new()
	}
}

// ########################################
// Transform stack
// ########################################

#[derive(Debug, Copy, Clone)]
pub struct Transform {
	pub rel_pos: Vec2,
	pub visual_dim: Vec2, // for convenience
	pub raw_dim: Vec2,    // for convenience
	pub abs_pos: Vec2,    // for convenience, will be set after pushing
	pub transform: glam::Mat4,
	pub transform_rel: glam::Mat4,
}

impl Default for Transform {
	fn default() -> Self {
		Self {
			abs_pos: Default::default(),
			rel_pos: Default::default(),
			visual_dim: Default::default(),
			raw_dim: Default::default(),
			transform: Mat4::IDENTITY,
			transform_rel: Default::default(),
		}
	}
}

impl<T> StackItem<T> for Transform where Transform: Pushable<T> {}

impl Pushable<Transform> for Transform {
	fn push(&mut self, upper: &Transform) {
		// fixme: there is definitely a better way to do these operations
		let translation_matrix = Mat4::from_translation(Vec3::new(self.rel_pos.x, self.rel_pos.y, 0.0));

		self.abs_pos = upper.abs_pos + self.rel_pos;
		let absolute_shift_matrix = Mat4::from_translation(Vec3::new(self.abs_pos.x, self.abs_pos.y, 0.0));
		let absolute_shift_matrix_neg = Mat4::from_translation(Vec3::new(-self.abs_pos.x, -self.abs_pos.y, 0.0));

		self.transform =
			(absolute_shift_matrix * self.transform * absolute_shift_matrix_neg) * upper.transform * translation_matrix;
	}
}

pub type TransformStack = GenericStack<Transform, 64>;

// ########################################
// Scissor stack
// ########################################

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScissorBoundary(pub drawing::Boundary);

impl Default for ScissorBoundary {
	fn default() -> Self {
		Self(drawing::Boundary {
			pos: Default::default(),
			size: Vec2::splat(1.0e12),
		})
	}
}

impl<T> StackItem<T> for ScissorBoundary where ScissorBoundary: Pushable<T> {}

impl Pushable<ScissorBoundary> for ScissorBoundary {
	fn push(&mut self, upper: &ScissorBoundary) {
		let mut display_pos = self.0.pos;
		let mut display_size = self.0.size;

		let upper = &upper.0;

		// limit in x-coord
		if display_pos.x < upper.left() {
			display_size.x -= upper.left() - display_pos.x;
			display_pos.x = upper.left();
		}

		// limit in y-coord
		if display_pos.y < upper.top() {
			display_size.y -= upper.top() - display_pos.y;
			display_pos.y = upper.top();
		}

		// limit in width
		if display_pos.x + display_size.x > upper.right() {
			display_size.x = upper.right() - display_pos.x;
		}

		// limit in height
		if display_pos.y + display_size.y > upper.bottom() {
			display_size.y = upper.bottom() - display_pos.y;
		}

		self.0.pos = display_pos;
		self.0.size = display_size;
	}
}

pub type ScissorStack = GenericStack<ScissorBoundary, 64>;

impl ScissorStack {
	pub const fn is_out_of_bounds(&self) -> bool {
		let boundary = &self.get().0;
		boundary.width() < 0.0 || boundary.height() < 0.0
	}
}
