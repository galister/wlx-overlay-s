use std::{cell::RefCell, rc::Rc};

use cosmic_text::{Attrs, Buffer, Metrics, Shaping, Wrap};
use taffy::AvailableSpace;

use crate::{
	drawing::{self, Boundary},
	renderer_vk::text::{FONT_SYSTEM, TextStyle},
};

use super::{WidgetObj, WidgetState};

#[derive(Default)]
pub struct TextParams {
	pub content: String,
	pub style: TextStyle,
}

pub struct TextLabel {
	params: TextParams,
	buffer: Rc<RefCell<Buffer>>,
	last_boundary: Boundary,
}

impl TextLabel {
	pub fn create(params: TextParams) -> anyhow::Result<WidgetState> {
		let metrics = Metrics::from(&params.style);
		let attrs = Attrs::from(&params.style);
		let wrap = Wrap::from(&params.style);

		let mut buffer = Buffer::new_empty(metrics);
		{
			let mut font_system = FONT_SYSTEM.lock();
			let mut buffer = buffer.borrow_with(&mut font_system);
			buffer.set_wrap(wrap);

			buffer.set_rich_text(
				[(params.content.as_str(), attrs)],
				&Attrs::new(),
				Shaping::Advanced,
				params.style.align.map(|a| a.into()),
			);
		}

		WidgetState::new(Box::new(Self {
			params,
			buffer: Rc::new(RefCell::new(buffer)),
			last_boundary: Boundary::default(),
		}))
	}

	pub fn set_text(&mut self, text: &str) {
		if self.params.content.as_str() == text {
			return;
		}

		self.params.content = String::from(text);
		let attrs = Attrs::from(&self.params.style);
		let mut font_system = FONT_SYSTEM.lock();

		let mut buffer = self.buffer.borrow_mut();
		buffer.set_rich_text(
			&mut font_system,
			[(self.params.content.as_str(), attrs)],
			&Attrs::new(),
			Shaping::Advanced,
			self.params.style.align.map(|a| a.into()),
		);
	}

	pub fn get_text(&self) -> &str {
		&self.params.content
	}
}

impl WidgetObj for TextLabel {
	fn draw(&mut self, state: &mut super::DrawState, _params: &super::DrawParams) {
		let boundary = drawing::Boundary::construct(state.transform_stack);

		if self.last_boundary != boundary {
			self.last_boundary = boundary;
			let mut font_system = FONT_SYSTEM.lock();
			let mut buffer = self.buffer.borrow_mut();
			buffer.set_size(
				&mut font_system,
				Some(boundary.size.x),
				Some(boundary.size.y),
			);
		}

		state.primitives.push(drawing::RenderPrimitive {
			boundary,
			depth: state.depth,
			payload: drawing::PrimitivePayload::Text(self.buffer.clone()),
			transform: state.transform_stack.get().transform,
		});
	}

	fn measure(
		&mut self,
		known_dimensions: taffy::Size<Option<f32>>,
		available_space: taffy::Size<taffy::AvailableSpace>,
	) -> taffy::Size<f32> {
		// Set width constraint
		let width_constraint = known_dimensions.width.or(match available_space.width {
			AvailableSpace::MinContent => Some(0.0),
			AvailableSpace::MaxContent => None,
			AvailableSpace::Definite(width) => Some(width),
		});

		let mut font_system = FONT_SYSTEM.lock();
		let mut buffer = self.buffer.borrow_mut();

		buffer.set_size(&mut font_system, width_constraint, None);

		// Determine measured size of text
		let (width, total_lines) = buffer
			.layout_runs()
			.fold((0.0, 0usize), |(width, total_lines), run| {
				(run.line_w.max(width), total_lines + 1)
			});
		let height = total_lines as f32 * buffer.metrics().line_height;
		taffy::Size { width, height }
	}
}
