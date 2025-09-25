use glam::Vec2;

use crate::drawing;

pub trait Pushable<T> {
	fn push(&mut self, item: &T);
}

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

#[derive(Default, Copy, Clone)]
pub struct Transform {
	pub pos: Vec2,
	pub transform: glam::Mat4,
	pub dim: Vec2, // for convenience
}

impl<T> StackItem<T> for Transform where Transform: Pushable<T> {}

impl Pushable<Transform> for Transform {
	fn push(&mut self, upper: &Transform) {
		self.pos += upper.pos;
	}
}

pub type TransformStack = GenericStack<Transform, 64>;

// ########################################
// Scissor stack
// ########################################

impl<T> StackItem<T> for drawing::Boundary where drawing::Boundary: Pushable<T> {}

impl Pushable<drawing::Boundary> for drawing::Boundary {
	fn push(&mut self, upper: &drawing::Boundary) {
		let mut display_pos = self.pos;
		let mut display_size = self.size;

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

		self.pos = display_pos;
		self.size = display_size;
	}
}

pub type ScissorStack = GenericStack<drawing::Boundary, 64>;
