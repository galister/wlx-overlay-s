use taffy::TaffyTree;

use crate::{
	any::AnyTrait,
	event::EventAlterables,
	layout::{WidgetID, WidgetMap},
};

pub mod button;
pub mod slider;

pub struct InitData<'a> {
	pub alterables: &'a mut EventAlterables,
	pub widgets: &'a WidgetMap,
	pub tree: &'a TaffyTree<WidgetID>,
}

pub trait Component: AnyTrait {
	fn init(&self, data: &mut InitData);
}
