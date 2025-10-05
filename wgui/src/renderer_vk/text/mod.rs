pub mod custom_glyph;
mod shaders;
pub mod text_atlas;
pub mod text_renderer;

use std::{cell::RefCell, rc::Rc, sync::LazyLock};

use cosmic_text::{Align, Attrs, Buffer, Color, FontSystem, Metrics, Style, SwashCache, Weight, Wrap};
use custom_glyph::{ContentType, CustomGlyph};
use etagere::AllocId;
use glam::Mat4;
use parking_lot::Mutex;

use crate::drawing::{self};

pub static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> = LazyLock::new(|| Mutex::new(FontSystem::new()));
pub static SWASH_CACHE: LazyLock<Mutex<SwashCache>> = LazyLock::new(|| Mutex::new(SwashCache::new()));

/// Used in case no `font_size` is defined
const DEFAULT_FONT_SIZE: f32 = 14.;

/// In case no `line_height` is defined, use `font_size` * `DEFAULT_LINE_HEIGHT_RATIO`
const DEFAULT_LINE_HEIGHT_RATIO: f32 = 1.43;

pub(crate) const DEFAULT_METRICS: Metrics =
	Metrics::new(DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE * DEFAULT_LINE_HEIGHT_RATIO);

#[derive(Debug, Clone)]
pub struct TextShadow {
	pub y: f32,
	pub x: f32,
	pub color: drawing::Color,
}

impl Default for TextShadow {
	fn default() -> Self {
		Self {
			y: 1.5,
			x: 1.5,
			color: drawing::Color::default(),
		}
	}
}

#[derive(Debug, Default, Clone)]
pub struct TextStyle {
	pub size: Option<f32>,
	pub line_height: Option<f32>,
	pub color: Option<drawing::Color>,
	pub style: Option<FontStyle>,
	pub weight: Option<FontWeight>,
	pub align: Option<HorizontalAlign>,
	pub wrap: bool,
	pub shadow: Option<TextShadow>,
}

impl From<&TextStyle> for Attrs<'_> {
	fn from(style: &TextStyle) -> Self {
		Attrs::new()
			.color(style.color.unwrap_or_default().into())
			.style(style.style.unwrap_or_default().into())
			.weight(style.weight.unwrap_or_default().into())
	}
}

impl From<&TextStyle> for Metrics {
	fn from(style: &TextStyle) -> Self {
		let font_size = style.size.unwrap_or(DEFAULT_FONT_SIZE);

		Self {
			font_size,
			line_height: style
				.size
				.unwrap_or_else(|| (font_size * DEFAULT_LINE_HEIGHT_RATIO).round()),
		}
	}
}

impl From<&TextStyle> for Wrap {
	fn from(value: &TextStyle) -> Self {
		if value.wrap { Self::WordOrGlyph } else { Self::None }
	}
}

// helper structs for serde

#[derive(Default, Debug, Clone, Copy)]
pub enum FontStyle {
	#[default]
	Normal,
	Italic,
}

impl From<FontStyle> for Style {
	fn from(value: FontStyle) -> Self {
		match value {
			FontStyle::Normal => Self::Normal,
			FontStyle::Italic => Self::Italic,
		}
	}
}

#[derive(Default, Debug, Clone, Copy)]
pub enum FontWeight {
	#[default]
	Normal,
	Bold,
}

impl From<FontWeight> for Weight {
	fn from(value: FontWeight) -> Self {
		match value {
			FontWeight::Normal => Self::NORMAL,
			FontWeight::Bold => Self::BOLD,
		}
	}
}

#[derive(Default, Debug, Clone, Copy)]
pub enum HorizontalAlign {
	#[default]
	Left,
	Right,
	Center,
	Justified,
	End,
}

impl From<HorizontalAlign> for Align {
	fn from(value: HorizontalAlign) -> Self {
		match value {
			HorizontalAlign::Left => Self::Left,
			HorizontalAlign::Right => Self::Right,
			HorizontalAlign::Center => Self::Center,
			HorizontalAlign::Justified => Self::Justified,
			HorizontalAlign::End => Self::End,
		}
	}
}

impl From<drawing::Color> for cosmic_text::Color {
	fn from(value: drawing::Color) -> Self {
		Self::rgba(
			(value.r * 255.999) as _,
			(value.g * 255.999) as _,
			(value.b * 255.999) as _,
			(value.a * 255.999) as _,
		)
	}
}

impl From<cosmic_text::Color> for drawing::Color {
	fn from(value: cosmic_text::Color) -> Self {
		Self::new(
			f32::from(value.r()) / 255.999,
			f32::from(value.g()) / 255.999,
			f32::from(value.b()) / 255.999,
			f32::from(value.a()) / 255.999,
		)
	}
}

// glyphon types below

pub(super) enum GpuCacheStatus {
	InAtlas { x: u16, y: u16, content_type: ContentType },
	SkipRasterization,
}

pub(super) struct GlyphDetails {
	width: u16,
	height: u16,
	gpu_cache: GpuCacheStatus,
	atlas_id: Option<AllocId>,
	top: i16,
	left: i16,
}

/// Controls the visible area of the text. Any text outside of the visible area will be clipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
	/// The position of the left edge of the visible area.
	pub left: i32,
	/// The position of the top edge of the visible area.
	pub top: i32,
	/// The position of the right edge of the visible area.
	pub right: i32,
	/// The position of the bottom edge of the visible area.
	pub bottom: i32,
}

/// The default visible area doesn't clip any text.
impl Default for TextBounds {
	fn default() -> Self {
		Self {
			left: i32::MIN,
			top: i32::MIN,
			right: i32::MAX,
			bottom: i32::MAX,
		}
	}
}

/// A text area containing text to be rendered along with its overflow behavior.
#[derive(Clone)]
pub struct TextArea<'a> {
	/// The buffer containing the text to be rendered.
	pub buffer: Rc<RefCell<Buffer>>,
	/// The left edge of the buffer.
	pub left: f32,
	/// The top edge of the buffer.
	pub top: f32,
	/// The scaling to apply to the buffer.
	pub scale: f32,
	/// The visible bounds of the text area. This is used to clip the text and doesn't have to
	/// match the `left` and `top` values.
	pub bounds: TextBounds,
	/// The default color of the text area.
	pub default_color: Color,
	/// Override text color. Used for shadow.
	pub override_color: Option<Color>,
	/// Additional custom glyphs to render.
	pub custom_glyphs: &'a [CustomGlyph],
	/// Text transformation
	pub transform: Mat4,
}
