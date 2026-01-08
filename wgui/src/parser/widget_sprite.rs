use crate::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{parse_children, parse_widget_universal, style::parse_style, AttribPair, ParserContext, ParserFile},
	renderer_vk::text::custom_glyph::CustomGlyphData,
	widget::sprite::{WidgetSprite, WidgetSpriteParams},
};

use super::parse_color_hex;

pub fn parse_widget_sprite<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
	tag_name: &str,
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetSpriteParams::default();
	let style = parse_style(ctx, attribs, tag_name);

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
					glyph = match CustomGlyphData::from_assets(&ctx.layout.state.globals, asset_path) {
						Ok(glyph) => Some(glyph),
						Err(e) => {
							log::warn!("failed to load {value}: {e}");
							None
						}
					}
				}
			}
			"color" => {
				if let Some(color) = parse_color_hex(value) {
					params.color = Some(color);
				} else {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			}
			_ => {}
		}
	}

	if let Some(glyph) = glyph {
		params.glyph_data = Some(glyph);
	} else {
		ctx.print_missing_attrib(tag_name, "src");
	}

	let (widget, _) = ctx.layout.add_child(parent_id, WidgetSprite::create(params), style)?;

	parse_widget_universal(ctx, &widget, attribs, tag_name);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
