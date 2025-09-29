use crate::{
	layout::WidgetID,
	parser::{parse_children, parse_widget_universal, style::parse_style, AttribPair, ParserContext, ParserFile},
	renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
	widget::sprite::{WidgetSprite, WidgetSpriteParams},
};

use super::{parse_color_hex, print_invalid_attrib};

pub fn parse_widget_sprite<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetSpriteParams::default();
	let style = parse_style(&attribs);

	let mut glyph = None;
	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"src" => {
				if !value.is_empty() {
					glyph = match CustomGlyphContent::from_assets(&mut ctx.layout.state.globals.assets(), value) {
						Ok(glyph) => Some(glyph),
						Err(e) => {
							log::warn!("failed to load {value}: {e}");
							None
						}
					}
				}
			}
			"src_ext" => {
				if !value.is_empty() && std::fs::exists(value).unwrap_or(false) {
					glyph = CustomGlyphContent::from_file(value).ok();
				}
			}
			"color" => {
				if let Some(color) = parse_color_hex(value) {
					params.color = Some(color);
				} else {
					print_invalid_attrib(key, value);
				}
			}
			_ => {}
		}
	}

	if let Some(glyph) = glyph {
		params.glyph_data = Some(CustomGlyphData::new(glyph));
	} else {
		log::warn!("No source for sprite node!");
	}

	let (new_id, _) = ctx.layout.add_child(parent_id, WidgetSprite::create(params), style)?;

	parse_widget_universal(ctx, new_id, attribs);
	parse_children(file, ctx, node, new_id)?;

	Ok(new_id)
}
