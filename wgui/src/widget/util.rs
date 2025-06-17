#[derive(Clone, Copy, Debug)]
pub enum WLength {
	Units(f32),
	Percent(f32), // 0.0 - 1.0
}

impl Default for WLength {
	fn default() -> Self {
		Self::Units(0.0)
	}
}
