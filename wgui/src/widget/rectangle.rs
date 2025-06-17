use crate::{
	drawing::{self, GradientMode},
	widget::util::WLength,
};

use super::{WidgetObj, WidgetState};

#[derive(Default)]
pub struct RectangleParams {
	pub color: drawing::Color,
	pub color2: drawing::Color,
	pub gradient: GradientMode,

	pub border: f32,
	pub border_color: drawing::Color,

	pub round: WLength,
}

pub struct Rectangle {
	pub params: RectangleParams,
}

impl Rectangle {
	pub fn create(params: RectangleParams) -> anyhow::Result<WidgetState> {
		WidgetState::new(Box::new(Rectangle { params }))
	}
}

impl WidgetObj for Rectangle {
	fn draw(&mut self, state: &mut super::DrawState, _params: &super::DrawParams) {
		let boundary = drawing::Boundary::construct(state.transform_stack);

		let round_units = match self.params.round {
			WLength::Units(units) => units as u8,
			WLength::Percent(percent) => {
				(f32::min(boundary.size.x, boundary.size.y) * percent / 2.0) as u8
			}
		};

		state.primitives.push(drawing::RenderPrimitive {
			boundary,
			depth: state.depth,
			transform: state.transform_stack.get().transform,
			payload: drawing::PrimitivePayload::Rectangle(drawing::Rectangle {
				color: self.params.color,
				color2: self.params.color2,
				gradient: self.params.gradient,
				border: self.params.border,
				border_color: self.params.border_color,
				round_units,
			}),
		});
	}
}
