use crate::{any::AnyTrait, event::EventAlterables, layout::LayoutState};

pub mod button;
pub mod slider;

pub struct InitData<'a> {
	pub state: &'a LayoutState,
	pub alterables: &'a mut EventAlterables,
}

pub trait Component: AnyTrait {
	fn init(&self, data: &mut InitData);
}
