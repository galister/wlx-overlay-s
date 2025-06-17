use super::{WidgetObj, WidgetState};

pub struct Div {}

impl Div {
	pub fn create() -> anyhow::Result<WidgetState> {
		WidgetState::new(Box::new(Self {}))
	}
}

impl WidgetObj for Div {
	fn draw(&mut self, _state: &mut super::DrawState, _params: &super::DrawParams) {
		// no-op
	}
}
