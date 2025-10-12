use crate::{
	assets::AssetPath,
	layout::WidgetID,
	parser::{AttribPair, ParserContext, ParserFile, parse_children, parse_widget_universal, style::parse_style},
	renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
	widget::sprite::{WidgetSprite, WidgetSpriteParams},
};

use super::{parse_color_hex, print_invalid_attrib};

pub fn parse_widget_sprite<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetSpriteParams::default();
	let style = parse_style(attribs);

	let mut glyph = None;
	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"src" | "src_ext" | "src_internal" => {
				let asset_path = match key {
					"src" => AssetPath::BuiltIn(value),
					"src_ext" => AssetPath::Filesystem(value),
					"src_internal" => AssetPath::WguiInternal(value),
					_ => unreachable!(),
				};

				if !value.is_empty() {
					glyph = match CustomGlyphContent::from_assets(&mut ctx.layout.state.globals, asset_path) {
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

	let (widget, _) = ctx.layout.add_child(parent_id, WidgetSprite::create(params), style)?;

	parse_widget_universal(ctx, widget.id, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}