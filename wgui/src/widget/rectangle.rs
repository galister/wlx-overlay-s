use slotmap::Key;

use crate::{
	drawing::{self, GradientMode, PrimitiveExtent},
	layout::WidgetID,
	widget::{WidgetStateFlags, util::WLength},
};

use super::{WidgetObj, WidgetState};

#[derive(Debug, Default)]
pub struct WidgetRectangleParams {
	pub color: drawing::Color,
	pub color2: drawing::Color,
	pub gradient: GradientMode,

	pub border: f32,
	pub border_color: drawing::Color,

	pub round: WLength,
}

pub struct WidgetRectangle {
	pub params: WidgetRectangleParams,
	id: WidgetID,
}

impl WidgetRectangle {
	pub fn create(params: WidgetRectangleParams) -> WidgetState {
		WidgetState::new(
			WidgetStateFlags::default(),
			Box::new(Self {
				params,
				id: WidgetID::null(),
			}),
		)
	}
}

impl WidgetObj for WidgetRectangle {
	fn draw(&mut self, state: &mut super::DrawState, _params: &super::DrawParams) {
		let boundary = drawing::Boundary::construct_relative(state.transform_stack);

		let round_units = match self.params.round {
			WLength::Units(units) => units as u8,
			WLength::Percent(percent) => (f32::min(boundary.size.x, boundary.size.y) * percent / 2.0) as u8,
		};

		state.primitives.push(drawing::RenderPrimitive::Rectangle(
			PrimitiveExtent {
				boundary,
				transform: state.transform_stack.get().transform,
			},
			drawing::Rectangle {
				color: self.params.color,
				color2: self.params.color2,
				gradient: self.params.gradient,
				border: self.params.border,
				border_color: self.params.border_color,
				round_units,
			},
		));
	}

	fn get_id(&self) -> WidgetID {
		self.id
	}

	fn set_id(&mut self, id: WidgetID) {
		self.id = id;
	}

	fn get_type(&self) -> super::WidgetType {
		super::WidgetType::Rectangle
	}

	fn debug_print(&self) -> String {
		format!(
			"[color: {}][color2: {}][gradient: {:?}]",
			self.params.color.debug_ansi_block(),
			self.params.color2.debug_ansi_block(),
			self.params.gradient,
		)
	}
}
