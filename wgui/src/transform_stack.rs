use glam::Vec2;

#[derive(Default, Copy, Clone)]
pub struct Transform {
	pub pos: Vec2,
	pub transform: glam::Mat4,

	pub dim: Vec2, // for convenience
}

const TRANSFORM_STACK_MAX: usize = 64;
pub struct TransformStack {
	pub stack: [Transform; TRANSFORM_STACK_MAX],
	top: u8,
}

impl TransformStack {
	pub fn new() -> Self {
		Self {
			stack: [Default::default(); TRANSFORM_STACK_MAX],
			top: 1,
		}
	}

	pub fn push(&mut self, mut t: Transform) {
		assert!(self.top < TRANSFORM_STACK_MAX as u8);
		let idx = (self.top - 1) as usize;
		t.pos += self.stack[idx].pos;
		self.stack[self.top as usize] = t;
		self.top += 1;
	}

	pub fn pop(&mut self) {
		assert!(self.top > 0);
		self.top -= 1;
	}

	pub const fn get(&self) -> &Transform {
		&self.stack[(self.top - 1) as usize]
	}

	pub const fn get_pos(&self) -> Vec2 {
		self.stack[(self.top - 1) as usize].pos
	}
}

impl Default for TransformStack {
	fn default() -> Self {
		Self::new()
	}
}
