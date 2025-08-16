use super::{WidgetObj, WidgetState};

pub struct WidgetDiv {}

impl WidgetDiv {
	pub fn create() -> WidgetState {
		WidgetState::new(Box::new(Self {}))
	}
}

impl WidgetObj for WidgetDiv {
	fn draw(&mut self, _state: &mut super::DrawState, _params: &super::DrawParams) {
		// no-op
	}
}
