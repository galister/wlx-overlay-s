use crate::{
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_universal,
		style::style_from_node,
	},
	renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
	widget::sprite::{SpriteBox, SpriteBoxParams},
};

use super::{parse_color_hex, print_invalid_attrib};

pub fn parse_widget_sprite<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = SpriteBoxParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	let mut glyph = None;
	for (key, value) in attribs {
		match key.as_ref() {
			"src" => {
				glyph = match CustomGlyphContent::from_assets(&mut ctx.layout.assets, &value) {
					Ok(glyph) => Some(glyph),
					Err(e) => {
						log::warn!("failed to load {}: {}", value, e);
						None
					}
				}
			}
			"src_ext" => {
				if std::fs::exists(value.as_ref()).unwrap_or(false) {
					glyph = CustomGlyphContent::from_file(&value).ok();
				}
			}
			"color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.color = Some(color);
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			_ => {}
		}
	}

	if let Some(glyph) = glyph {
		params.glyph_data = Some(CustomGlyphData::new(glyph));
	} else {
		log::warn!("No source for sprite node!");
	};

	let style = style_from_node(file, ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, SpriteBox::create(params)?, style)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
