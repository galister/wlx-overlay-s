use crate::{
	gfx::{cmd::GfxCommandBuffer, pass::WGfxPass},
	renderer_vk::{model_buffer::ModelBuffer, viewport::Viewport},
};

use super::{
	ContentType, FontSystem, GlyphDetails, GpuCacheStatus, SwashCache, TextArea,
	custom_glyph::{CustomGlyphCacheKey, RasterizeCustomGlyphRequest, RasterizedCustomGlyph},
	text_atlas::{GlyphVertex, TextAtlas, TextPipeline},
};
use cosmic_text::{Color, SubpixelBin, SwashContent};
use glam::{Mat4, Vec2, Vec3};
use vulkano::{
	buffer::{BufferUsage, Subbuffer},
	command_buffer::CommandBufferUsage,
};

struct CachedPass {
	pass: WGfxPass<GlyphVertex>,
	res: [u32; 2],
}

/// A text renderer that uses cached glyphs to render text into an existing render pass.
pub struct TextRenderer {
	pipeline: TextPipeline,
	vertex_buffer: Subbuffer<[GlyphVertex]>,
	vertex_buffer_capacity: usize,
	glyph_vertices: Vec<GlyphVertex>,
	model_buffer: ModelBuffer,
	pass: Option<CachedPass>,
}

impl TextRenderer {
	/// Creates a new `TextRenderer`.
	pub fn new(atlas: &mut TextAtlas) -> anyhow::Result<Self> {
		// A buffer element is a single quad with a glyph on it
		const INITIAL_CAPACITY: usize = 256;

		let vertex_buffer = atlas.common.gfx.empty_buffer(
			BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
			INITIAL_CAPACITY as _,
		)?;

		Ok(Self {
			model_buffer: ModelBuffer::new(&atlas.common.gfx)?,
			pipeline: atlas.common.clone(),
			vertex_buffer,
			vertex_buffer_capacity: INITIAL_CAPACITY,
			glyph_vertices: Vec::new(),
			pass: None,
		})
	}

	/// Prepares all of the provided text areas for rendering.
	pub fn prepare<'a>(
		&mut self,
		font_system: &mut FontSystem,
		atlas: &mut TextAtlas,
		viewport: &Viewport,
		text_areas: impl IntoIterator<Item = TextArea<'a>>,
		cache: &mut SwashCache,
	) -> anyhow::Result<()> {
		self.glyph_vertices.clear();

		let resolution = viewport.resolution();

		for text_area in text_areas {
			let bounds_min_x = text_area.bounds.left.max(0);
			let bounds_min_y = text_area.bounds.top.max(0);
			let bounds_max_x = text_area.bounds.right.min(resolution[0] as i32);
			let bounds_max_y = text_area.bounds.bottom.min(resolution[1] as i32);

			for glyph in text_area.custom_glyphs.iter() {
				let x = text_area.left + (glyph.left * text_area.scale);
				let y = text_area.top + (glyph.top * text_area.scale);
				let width = (glyph.width * text_area.scale).round() as u16;
				let height = (glyph.height * text_area.scale).round() as u16;

				let (x, y, x_bin, y_bin) = if glyph.snap_to_physical_pixel {
					(
						x.round() as i32,
						y.round() as i32,
						SubpixelBin::Zero,
						SubpixelBin::Zero,
					)
				} else {
					let (x, x_bin) = SubpixelBin::new(x);
					let (y, y_bin) = SubpixelBin::new(y);
					(x, y, x_bin, y_bin)
				};

				let (cached_width, cached_height) = glyph.data.dim_for_cache_key(width, height);

				let cache_key = GlyphonCacheKey::Custom(CustomGlyphCacheKey {
					glyph_id: glyph.data.id,
					width: cached_width,
					height: cached_height,
					x_bin,
					y_bin,
				});

				let color = glyph.color.unwrap_or(text_area.default_color);

				if let Some(glyph_to_render) = prepare_glyph(
					PrepareGlyphParams {
						label_pos: Vec2::new(text_area.left, text_area.top),
						x,
						y,
						line_y: 0.0,
						color,
						cache_key,
						atlas,
						cache,
						font_system,
						model_buffer: &mut self.model_buffer,
						scale_factor: text_area.scale,
						glyph_scale: width as f32 / cached_width as f32,
						bounds_min_x,
						bounds_min_y,
						bounds_max_x,
						bounds_max_y,
						depth: text_area.depth,
						transform: &text_area.transform,
					},
					|_cache, _font_system| -> Option<GetGlyphImageResult> {
						if cached_width == 0 || cached_height == 0 {
							return None;
						}

						let input = RasterizeCustomGlyphRequest {
							data: glyph.data.clone(),
							width: cached_width,
							height: cached_height,
							x_bin,
							y_bin,
							scale: text_area.scale,
						};

						let output = RasterizedCustomGlyph::try_from(&input)?;

						output.validate(&input, None);

						Some(GetGlyphImageResult {
							content_type: output.content_type,
							top: 0,
							left: 0,
							width: output.width,
							height: output.height,
							data: output.data,
						})
					},
				)? {
					self.glyph_vertices.push(glyph_to_render);
				}
			}

			let is_run_visible = |run: &cosmic_text::LayoutRun| {
				let start_y_physical = (text_area.top + (run.line_top * text_area.scale)) as i32;
				let end_y_physical = start_y_physical + (run.line_height * text_area.scale) as i32;

				start_y_physical <= text_area.bounds.bottom && text_area.bounds.top <= end_y_physical
			};

			let buffer = text_area.buffer.borrow();

			let layout_runs = buffer
				.layout_runs()
				.skip_while(|run| !is_run_visible(run))
				.take_while(is_run_visible);

			for run in layout_runs {
				for glyph in run.glyphs.iter() {
					let physical_glyph = glyph.physical((text_area.left, text_area.top), text_area.scale);

					let color = match glyph.color_opt {
						Some(some) => some,
						None => text_area.default_color,
					};

					if let Some(glyph_to_render) = prepare_glyph(
						PrepareGlyphParams {
							label_pos: Vec2::new(text_area.left, text_area.top),
							x: physical_glyph.x,
							y: physical_glyph.y,
							line_y: run.line_y,
							color,
							cache_key: GlyphonCacheKey::Text(physical_glyph.cache_key),
							atlas,
							cache,
							font_system,
							model_buffer: &mut self.model_buffer,
							glyph_scale: 1.0,
							scale_factor: text_area.scale,
							bounds_min_x,
							bounds_min_y,
							bounds_max_x,
							bounds_max_y,
							depth: text_area.depth,
							transform: &text_area.transform,
						},
						|cache, font_system| -> Option<GetGlyphImageResult> {
							let image = cache.get_image_uncached(font_system, physical_glyph.cache_key)?;

							let content_type = match image.content {
								SwashContent::Color => ContentType::Color,
								SwashContent::Mask => ContentType::Mask,
								SwashContent::SubpixelMask => {
									// Not implemented yet, but don't panic if this happens.
									ContentType::Mask
								}
							};

							Some(GetGlyphImageResult {
								content_type,
								top: image.placement.top as i16,
								left: image.placement.left as i16,
								width: image.placement.width as u16,
								height: image.placement.height as u16,
								data: image.data,
							})
						},
					)? {
						self.glyph_vertices.push(glyph_to_render);
					}
				}
			}
		}

		let will_render = !self.glyph_vertices.is_empty();
		if !will_render {
			return Ok(());
		}

		let vertices = self.glyph_vertices.as_slice();

		while self.vertex_buffer_capacity < vertices.len() {
			let new_capacity = self.vertex_buffer_capacity * 2;
			self.vertex_buffer = self.pipeline.gfx.empty_buffer(
				BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
				new_capacity as _,
			)?;
			self.vertex_buffer_capacity = new_capacity;
		}
		self.vertex_buffer.write()?[..vertices.len()].clone_from_slice(vertices);

		Ok(())
	}

	/// Renders all layouts that were previously provided to `prepare`.
	pub fn render(
		&mut self,
		atlas: &TextAtlas,
		viewport: &mut Viewport,
		cmd_buf: &mut GfxCommandBuffer,
	) -> anyhow::Result<()> {
		if self.glyph_vertices.is_empty() {
			return Ok(());
		}

		let res = viewport.resolution();
		self.model_buffer.upload(&atlas.common.gfx)?;

		let cache = match self.pass.take() {
			Some(p) if p.res == res => p,
			_ => {
				let descriptor_sets = vec![
					atlas.color_atlas.image_descriptor.clone(),
					atlas.mask_atlas.image_descriptor.clone(),
					viewport.get_text_descriptor(&self.pipeline),
					self.model_buffer.get_text_descriptor(&self.pipeline),
				];

				let pass = self.pipeline.inner.create_pass(
					[res[0] as _, res[1] as _],
					self.vertex_buffer.clone(),
					0..4,
					0..self.glyph_vertices.len() as u32,
					descriptor_sets,
				)?;
				CachedPass { pass, res }
			}
		};

		cmd_buf.run_ref(&cache.pass)?;
		self.pass = Some(cache);
		Ok(())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum GlyphonCacheKey {
	Text(cosmic_text::CacheKey),
	Custom(CustomGlyphCacheKey),
}

struct GetGlyphImageResult {
	content_type: ContentType,
	top: i16,
	left: i16,
	width: u16,
	height: u16,
	data: Vec<u8>,
}

struct PrepareGlyphParams<'a> {
	label_pos: Vec2,
	x: i32,
	y: i32,
	line_y: f32,
	color: Color,
	cache_key: GlyphonCacheKey,
	atlas: &'a mut TextAtlas,
	cache: &'a mut SwashCache,
	font_system: &'a mut FontSystem,
	model_buffer: &'a mut ModelBuffer,
	transform: &'a Mat4,
	scale_factor: f32,
	glyph_scale: f32,
	bounds_min_x: i32,
	bounds_min_y: i32,
	bounds_max_x: i32,
	bounds_max_y: i32,
	depth: f32,
}

#[allow(clippy::too_many_arguments)]
fn prepare_glyph(
	par: PrepareGlyphParams,
	get_glyph_image: impl FnOnce(&mut SwashCache, &mut FontSystem) -> Option<GetGlyphImageResult>,
) -> anyhow::Result<Option<GlyphVertex>> {
	let gfx = par.atlas.common.gfx.clone();
	let details = if let Some(details) = par.atlas.mask_atlas.glyph_cache.get(&par.cache_key) {
		par.atlas.mask_atlas.glyphs_in_use.insert(par.cache_key);
		details
	} else if let Some(details) = par.atlas.color_atlas.glyph_cache.get(&par.cache_key) {
		par.atlas.color_atlas.glyphs_in_use.insert(par.cache_key);
		details
	} else {
		let Some(image) = (get_glyph_image)(par.cache, par.font_system) else {
			return Ok(None);
		};

		let should_rasterize = image.width > 0 && image.height > 0;

		let (gpu_cache, atlas_id, inner) = if should_rasterize {
			let mut inner = par.atlas.inner_for_content_mut(image.content_type);

			// Find a position in the packer
			let allocation = loop {
				match inner.try_allocate(image.width as usize, image.height as usize) {
					Some(a) => break a,
					None => {
						if !par
							.atlas
							.grow(par.font_system, par.cache, image.content_type)?
						{
							anyhow::bail!(
								"Atlas full. atlas: {:?} cache_key: {:?}",
								image.content_type,
								par.cache_key
							);
						}

						inner = par.atlas.inner_for_content_mut(image.content_type);
					}
				}
			};
			let atlas_min = allocation.rectangle.min;

			let mut cmd_buf = gfx.create_xfer_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

			cmd_buf.update_image(
				inner.image_view.image().clone(),
				&image.data,
				[atlas_min.x as _, atlas_min.y as _, 0],
				Some([image.width as _, image.height as _, 1]),
			)?;

			cmd_buf.build_and_execute_now()?; //TODO: do not wait for fence here

			(
				GpuCacheStatus::InAtlas {
					x: atlas_min.x as u16,
					y: atlas_min.y as u16,
					content_type: image.content_type,
				},
				Some(allocation.id),
				inner,
			)
		} else {
			let inner = &mut par.atlas.color_atlas;
			(GpuCacheStatus::SkipRasterization, None, inner)
		};

		inner.glyphs_in_use.insert(par.cache_key);
		// Insert the glyph into the cache and return the details reference
		inner
			.glyph_cache
			.get_or_insert(par.cache_key, || GlyphDetails {
				width: image.width,
				height: image.height,
				gpu_cache,
				atlas_id,
				top: image.top,
				left: image.left,
			})
	};

	let mut x = par.x + details.left as i32;
	let mut y = (par.line_y * par.scale_factor).round() as i32 + par.y - details.top as i32;

	let (mut atlas_x, mut atlas_y, content_type) = match details.gpu_cache {
		GpuCacheStatus::InAtlas { x, y, content_type } => (x, y, content_type),
		GpuCacheStatus::SkipRasterization => return Ok(None),
	};

	let mut glyph_width = details.width as i32;
	let mut glyph_height = details.height as i32;

	// Starts beyond right edge or ends beyond left edge
	let max_x = x + glyph_width;
	if x > par.bounds_max_x || max_x < par.bounds_min_x {
		return Ok(None);
	}

	// Starts beyond bottom edge or ends beyond top edge
	let max_y = y + glyph_height;
	if y > par.bounds_max_y || max_y < par.bounds_min_y {
		return Ok(None);
	}

	// Clip left ege
	if x < par.bounds_min_x {
		let right_shift = par.bounds_min_x - x;

		x = par.bounds_min_x;
		glyph_width = max_x - par.bounds_min_x;
		atlas_x += right_shift as u16;
	}

	// Clip right edge
	if x + glyph_width > par.bounds_max_x {
		glyph_width = par.bounds_max_x - x;
	}

	// Clip top edge
	if y < par.bounds_min_y {
		let bottom_shift = par.bounds_min_y - y;

		y = par.bounds_min_y;
		glyph_height = max_y - par.bounds_min_y;
		atlas_y += bottom_shift as u16;
	}

	// Clip bottom edge
	if y + glyph_height > par.bounds_max_y {
		glyph_height = par.bounds_max_y - y;
	}

	let mut model = Mat4::IDENTITY;

	// top-left text transform
	model *= Mat4::from_translation(Vec3::new(
		par.label_pos.x / par.scale_factor,
		par.label_pos.y / par.scale_factor,
		0.0,
	));

	model *= *par.transform;

	// per-character transform
	model *= Mat4::from_translation(Vec3::new(
		((x as f32) - par.label_pos.x) / par.scale_factor,
		((y as f32) - par.label_pos.y) / par.scale_factor,
		0.0,
	));

	model *= glam::Mat4::from_scale(Vec3::new(
		glyph_width as f32 / par.scale_factor,
		glyph_height as f32 / par.scale_factor,
		0.0,
	));

	let in_model_idx = par.model_buffer.register(&model);

	Ok(Some(GlyphVertex {
		in_model_idx,
		in_rect_dim: [glyph_width as u16, glyph_height as u16],
		in_uv: [atlas_x, atlas_y],
		in_color: par.color.0,
		in_content_type: [
			content_type as u16,
			0, // unused (TODO!)
		],
		depth: par.depth,
		scale: par.glyph_scale,
	}))
}
