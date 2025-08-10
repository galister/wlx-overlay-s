use crate::{
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_widget_universal,
		style::parse_style,
	},
	renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
	widget::sprite::{WidgetSprite, WidgetSpriteParams},
};

use super::{parse_color_hex, print_invalid_attrib};

pub fn parse_widget_sprite<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = WidgetSpriteParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	let style = parse_style(&attribs);

	let mut glyph = None;
	for (key, value) in attribs {
		match key.as_ref() {
			"src" => {
				glyph =
					match CustomGlyphContent::from_assets(&mut ctx.layout.state.globals.assets(), &value) {
						Ok(glyph) => Some(glyph),
						Err(e) => {
							log::warn!("failed to load {value}: {e}");
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

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, WidgetSprite::create(params)?, style)?;

	parse_widget_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
