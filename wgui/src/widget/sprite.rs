use std::{cell::RefCell, rc::Rc};

use cosmic_text::{Attrs, Buffer, Color, Shaping, Weight};

use crate::{
	drawing::{self},
	renderer_vk::text::{
		DEFAULT_METRICS, FONT_SYSTEM,
		custom_glyph::{CustomGlyph, CustomGlyphData},
	},
};

use super::{WidgetObj, WidgetState};

#[derive(Debug, Default)]
pub struct WidgetSpriteParams {
	pub glyph_data: Option<CustomGlyphData>,
	pub color: Option<drawing::Color>,
}

#[derive(Debug, Default)]
pub struct WidgetSprite {
	params: WidgetSpriteParams,
}

impl WidgetSprite {
	pub fn create(params: WidgetSpriteParams) -> WidgetState {
		WidgetState::new(Box::new(Self { params }))
	}
}

impl WidgetObj for WidgetSprite {
	fn draw(&mut self, state: &mut super::DrawState, _params: &super::DrawParams) {
		let boundary = drawing::Boundary::construct(state.transform_stack);

		if let Some(glyph_data) = self.params.glyph_data.as_ref() {
			let glyph = CustomGlyph {
				data: glyph_data.clone(),
				left: 0.0,
				top: 0.0,
				width: boundary.size.x,
				height: boundary.size.y,
				color: Some(
					self
						.params
						.color
						.map_or(cosmic_text::Color::rgb(255, 255, 255), Into::into),
				),
				snap_to_physical_pixel: true,
			};

			state.primitives.push(drawing::RenderPrimitive {
				boundary,
				depth: state.depth,
				payload: drawing::PrimitivePayload::Sprite(Some(glyph)),
				transform: state.transform_stack.get().transform,
			});
		} else {
			// Source not set or not available, display error text
			let mut buffer = Buffer::new_empty(DEFAULT_METRICS);

			{
				let mut font_system = FONT_SYSTEM.lock();
				let mut buffer = buffer.borrow_with(&mut font_system);
				let attrs = Attrs::new()
					.color(Color::rgb(255, 0, 255))
					.weight(Weight::BOLD);

				// set text last in order to avoid expensive re-shaping
				buffer.set_text("Error", &attrs, Shaping::Basic);
			}
			state.primitives.push(drawing::RenderPrimitive {
				boundary,
				depth: state.depth,
				payload: drawing::PrimitivePayload::Text(Rc::new(RefCell::new(buffer))),
				transform: state.transform_stack.get().transform,
			});
		}
	}

	fn measure(
		&mut self,
		_known_dimensions: taffy::Size<Option<f32>>,
		_available_space: taffy::Size<taffy::AvailableSpace>,
	) -> taffy::Size<f32> {
		taffy::Size::ZERO
	}
}
