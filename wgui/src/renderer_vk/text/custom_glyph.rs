use std::{
	f32,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

use cosmic_text::SubpixelBin;
use image::RgbaImage;
use resvg::usvg::{Options, Tree};

use crate::assets::AssetProvider;

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub enum CustomGlyphContent {
	Svg(Box<Tree>),
	Image(RgbaImage),
}

impl CustomGlyphContent {
	pub fn from_bin_svg(data: &[u8]) -> anyhow::Result<Self> {
		let tree = Tree::from_data(data, &Options::default())?;
		Ok(CustomGlyphContent::Svg(Box::new(tree)))
	}

	pub fn from_bin_raster(data: &[u8]) -> anyhow::Result<Self> {
		let image = image::load_from_memory(data)?.into_rgba8();
		Ok(CustomGlyphContent::Image(image))
	}

	pub fn from_assets(provider: &mut Box<dyn AssetProvider>, path: &str) -> anyhow::Result<Self> {
		let data = provider.load_from_path(path)?;
		if path.ends_with(".svg") || path.ends_with(".svgz") {
			Ok(CustomGlyphContent::from_bin_svg(&data)?)
		} else {
			Ok(CustomGlyphContent::from_bin_raster(&data)?)
		}
	}

	pub fn from_file(path: &str) -> anyhow::Result<Self> {
		let data = std::fs::read(path)?;
		if path.ends_with(".svg") || path.ends_with(".svgz") {
			Ok(CustomGlyphContent::from_bin_svg(&data)?)
		} else {
			Ok(CustomGlyphContent::from_bin_raster(&data)?)
		}
	}
}

#[derive(Debug, Clone)]
pub struct CustomGlyphData {
	pub(super) id: usize,
	pub(super) content: Arc<CustomGlyphContent>,
}

impl CustomGlyphData {
	pub fn new(content: CustomGlyphContent) -> Self {
		Self {
			id: AUTO_INCREMENT.fetch_add(1, Ordering::Relaxed),
			content: Arc::new(content),
		}
	}

	pub fn dim_for_cache_key(&self, width: u16, height: u16) -> (u16, u16) {
		const MAX_RASTER_DIM: u16 = 256;
		match self.content.as_ref() {
			CustomGlyphContent::Svg(..) => (
				width.next_power_of_two().min(MAX_RASTER_DIM),
				height.next_power_of_two().min(MAX_RASTER_DIM),
			),
			CustomGlyphContent::Image(image) => (image.width() as _, image.height() as _),
		}
	}
}

impl PartialEq for CustomGlyphData {
	fn eq(&self, other: &Self) -> bool {
		self.id.eq(&other.id)
	}
}

/// A custom glyph to render
#[derive(Debug, Clone, PartialEq)]
pub struct CustomGlyph {
	/// The unique identifier for this glyph
	pub data: CustomGlyphData,
	/// The position of the left edge of the glyph
	pub left: f32,
	/// The position of the top edge of the glyph
	pub top: f32,
	/// The width of the glyph
	pub width: f32,
	/// The height of the glyph
	pub height: f32,
	/// The color of this glyph (only relevant if the glyph is rendered with the
	/// type [`ContentType::Mask`])
	///
	/// Set to `None` to use [`crate::TextArea::default_color`].
	pub color: Option<cosmic_text::Color>,
	/// If `true`, then this glyph will be snapped to the nearest whole physical
	/// pixel and the resulting `SubpixelBin`'s in `RasterizationRequest` will always
	/// be `Zero` (useful for images and other large glyphs).
	pub snap_to_physical_pixel: bool,
}

impl CustomGlyph {
	pub fn new(data: CustomGlyphData) -> Self {
		Self {
			data,
			left: 0.0,
			top: 0.0,
			width: 0.0,
			height: 0.0,
			color: None,
			snap_to_physical_pixel: true,
		}
	}
}

/// A request to rasterize a custom glyph
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizeCustomGlyphRequest {
	/// The unique identifier of the glyph
	pub data: CustomGlyphData,
	/// The width of the glyph in physical pixels
	pub width: u16,
	/// The height of the glyph in physical pixels
	pub height: u16,
	/// Binning of fractional X offset
	///
	/// If `CustomGlyph::snap_to_physical_pixel` was set to `true`, then this
	/// will always be `Zero`.
	pub x_bin: SubpixelBin,
	/// Binning of fractional Y offset
	///
	/// If `CustomGlyph::snap_to_physical_pixel` was set to `true`, then this
	/// will always be `Zero`.
	pub y_bin: SubpixelBin,
	/// The scaling factor applied to the text area (Note that `width` and
	/// `height` are already scaled by this factor.)
	pub scale: f32,
}

/// A rasterized custom glyph
#[derive(Debug, Clone)]
pub struct RasterizedCustomGlyph {
	/// The raw image data
	pub data: Vec<u8>,
	/// The type of image data contained in `data`
	pub content_type: ContentType,
	pub width: u16,
	pub height: u16,
}

impl RasterizedCustomGlyph {
	pub(super) fn try_from(input: &RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph> {
		match input.data.content.as_ref() {
			CustomGlyphContent::Svg(tree) => rasterize_svg(tree, input),
			CustomGlyphContent::Image(data) => rasterize_image(data),
		}
	}

	pub(super) fn validate(
		&self,
		input: &RasterizeCustomGlyphRequest,
		expected_type: Option<ContentType>,
	) {
		if let Some(expected_type) = expected_type {
			assert_eq!(
				self.content_type, expected_type,
				"Custom glyph rasterizer must always produce the same content type for a given input. Expected {:?}, got {:?}. Input: {:?}",
				expected_type, self.content_type, input
			);
		}

		assert_eq!(
			self.data.len(),
			self.width as usize * self.height as usize * self.content_type.bytes_per_pixel(),
			"Invalid custom glyph rasterizer output. Expected data of length {}, got length {}. Input: {:?}",
			self.width as usize * self.height as usize * self.content_type.bytes_per_pixel(),
			self.data.len(),
			input,
		);
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CustomGlyphCacheKey {
	/// Font ID
	pub glyph_id: usize,
	/// Glyph width
	pub width: u16,
	/// Glyph height
	pub height: u16,
	/// Binning of fractional X offset
	pub x_bin: SubpixelBin,
	/// Binning of fractional Y offset
	pub y_bin: SubpixelBin,
}

/// The type of image data contained in a rasterized glyph
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ContentType {
	/// Each pixel contains 32 bits of rgba data
	Color,
	/// Each pixel contains a single 8 bit channel
	Mask,
}

impl ContentType {
	/// The number of bytes per pixel for this content type
	pub fn bytes_per_pixel(&self) -> usize {
		match self {
			Self::Color => 4,
			Self::Mask => 1,
		}
	}
}

fn rasterize_svg(
	tree: &Tree,
	input: &RasterizeCustomGlyphRequest,
) -> Option<RasterizedCustomGlyph> {
	// Calculate the scale based on the "glyph size".
	let svg_size = tree.size();
	let scale_x = input.width as f32 / svg_size.width();
	let scale_y = input.height as f32 / svg_size.height();

	let mut pixmap = resvg::tiny_skia::Pixmap::new(input.width as u32, input.height as u32)?;
	let mut transform = resvg::usvg::Transform::from_scale(scale_x, scale_y);

	// Offset the glyph by the subpixel amount.
	let offset_x = input.x_bin.as_float();
	let offset_y = input.y_bin.as_float();
	if offset_x != 0.0 || offset_y != 0.0 {
		transform = transform.post_translate(offset_x, offset_y);
	}

	resvg::render(tree, transform, &mut pixmap.as_mut());

	Some(RasterizedCustomGlyph {
		data: pixmap.data().to_vec(),
		content_type: ContentType::Color,
		width: input.width,
		height: input.height,
	})
}

fn rasterize_image(image: &RgbaImage) -> Option<RasterizedCustomGlyph> {
	Some(RasterizedCustomGlyph {
		data: image.to_vec(),
		content_type: ContentType::Color,
		width: image.width() as _,
		height: image.height() as _,
	})
}
