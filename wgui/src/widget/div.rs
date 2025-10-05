use slotmap::Key;

use crate::layout::WidgetID;

use super::{WidgetObj, WidgetState};

pub struct WidgetDiv {
	id: WidgetID,
}

impl WidgetDiv {
	pub fn create() -> WidgetState {
		WidgetState::new(Box::new(Self { id: WidgetID::null() }))
	}
}

impl WidgetObj for WidgetDiv {
	fn draw(&mut self, _state: &mut super::DrawState, _params: &super::DrawParams) {
		// no-op
	}

	fn get_id(&self) -> WidgetID {
		self.id
	}

	fn set_id(&mut self, id: WidgetID) {
		self.id = id;
	}

	fn get_type(&self) -> super::WidgetType {
		super::WidgetType::Div
	}

	fn debug_print(&self) -> String {
		String::default()
	}
}
