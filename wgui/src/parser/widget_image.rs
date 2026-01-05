use crate::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{
		AttribPair, ParserContext, ParserFile, parse_children, parse_widget_universal, print_invalid_attrib,
		style::{parse_color, parse_round, parse_style},
	},
	renderer_vk::text::custom_glyph::CustomGlyphData,
	widget::image::{WidgetImage, WidgetImageParams},
};

pub fn parse_widget_image<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetImageParams::default();
	let style = parse_style(attribs);
	let mut glyph = None;

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"src" | "src_ext" | "src_builtin" | "src_internal" => {
				let asset_path = match key {
					"src" => AssetPath::FileOrBuiltIn(value),
					"src_ext" => AssetPath::File(value),
					"src_builtin" => AssetPath::BuiltIn(value),
					"src_internal" => AssetPath::WguiInternal(value),
					_ => unreachable!(),
				};

				if !value.is_empty() {
					glyph = match CustomGlyphData::from_assets(&mut ctx.layout.state.globals, asset_path) {
						Ok(glyph) => Some(glyph),
						Err(e) => {
							log::warn!("failed to load {value}: {e}");
							None
						}
					}
				}
			}
			"round" => {
				parse_round(
					value,
					&mut params.round,
					ctx.doc_params.globals.get().defaults.rounding_mult,
				);
			}
			"border" => {
				params.border = value.parse().unwrap_or_else(|_| {
					print_invalid_attrib(key, value);
					0.0
				});
			}
			"border_color" => {
				parse_color(value, &mut params.border_color);
			}
			_ => {}
		}
	}

	if let Some(glyph) = glyph {
		params.glyph_data = Some(glyph);
	} else {
		log::warn!("No source for image node!");
	}

	let (widget, _) = ctx.layout.add_child(parent_id, WidgetImage::create(params), style)?;

	parse_widget_universal(ctx, &widget, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
