use crate::{
	assets::AssetPath,
	components::{Component, button, tooltip},
	drawing::Color,
	i18n::Translation,
	layout::WidgetID,
	parser::{
		AttribPair, ParserContext, ParserFile, parse_children, parse_f32, process_component,
		style::{parse_color_opt, parse_round, parse_style, parse_text_style},
	},
	widget::util::WLength,
};

#[allow(clippy::too_many_lines)]
pub fn parse_component_button<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
	tag_name: &str,
) -> anyhow::Result<WidgetID> {
	let mut color: Option<Color> = None;
	let mut border = 2.0;
	let mut border_color: Option<Color> = None;
	let mut hover_color: Option<Color> = None;
	let mut hover_border_color: Option<Color> = None;
	let mut round = WLength::Units(4.0);
	let mut tooltip: Option<Translation> = None;
	let mut tooltip_side: Option<tooltip::TooltipSide> = None;
	let mut sticky: bool = false;
	let mut long_press_time = 0.0;
	let mut sprite_src: Option<AssetPath> = None;

	let mut translation: Option<Translation> = None;

	let text_style = parse_text_style(ctx, attribs, tag_name);
	let style = parse_style(ctx, attribs, tag_name);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"text" => {
				if !value.is_empty() {
					translation = Some(Translation::from_raw_text(value));
				}
			}
			"translation" => {
				if !value.is_empty() {
					translation = Some(Translation::from_translation_key(value));
				}
			}
			"round" => {
				parse_round(
					ctx,
					tag_name,
					key,
					value,
					&mut round,
					ctx.doc_params.globals.get().defaults.rounding_mult,
				);
			}
			"color" => {
				parse_color_opt(ctx, tag_name, key, value, &mut color);
			}
			"border" => {
				ctx.parse_check_f32(tag_name, key, value, &mut border);
			}
			"border_color" => {
				parse_color_opt(ctx, tag_name, key, value, &mut border_color);
			}
			"hover_color" => {
				parse_color_opt(ctx, tag_name, key, value, &mut hover_color);
			}
			"hover_border_color" => {
				parse_color_opt(ctx, tag_name, key, value, &mut hover_border_color);
			}
			"sprite_src" | "sprite_src_ext" | "sprite_src_builtin" | "sprite_src_internal" => {
				let asset_path = match key {
					"sprite_src" => AssetPath::FileOrBuiltIn(value),
					"sprite_src_ext" => AssetPath::File(value),
					"sprite_src_builtin" => AssetPath::BuiltIn(value),
					"sprite_src_internal" => AssetPath::WguiInternal(value),
					_ => unreachable!(),
				};

				if !value.is_empty() {
					sprite_src = Some(asset_path);
				}
			}
			"tooltip" if !value.is_empty() => tooltip = Some(Translation::from_translation_key(value)),
			"tooltip_str" if !value.is_empty() => tooltip = Some(Translation::from_raw_text(value)),
			"tooltip_side" => {
				tooltip_side = match value {
					"left" => Some(tooltip::TooltipSide::Left),
					"right" => Some(tooltip::TooltipSide::Right),
					"top" => Some(tooltip::TooltipSide::Top),
					"bottom" => Some(tooltip::TooltipSide::Bottom),
					_ => {
						ctx.print_invalid_attrib(tag_name, key, value);
						None
					}
				}
			}
			"sticky" => {
				let mut sticky_i32 = 0;
				sticky = ctx.parse_check_i32(tag_name, key, value, &mut sticky_i32) && sticky_i32 == 1;
			}
			"long_press_time" => {
				long_press_time = parse_f32(value).unwrap_or(long_press_time);
			}
			_ => {}
		}
	}

	let (widget, button) = button::construct(
		&mut ctx.get_construct_essentials(parent_id),
		button::Params {
			color,
			border,
			border_color,
			hover_border_color,
			hover_color,
			text: translation,
			style,
			text_style,
			round,
			tooltip: tooltip.map(|text| tooltip::TooltipInfo {
				side: tooltip_side.map_or(tooltip::TooltipSide::Top, |f| f),
				text,
			}),
			sticky,
			long_press_time,
			sprite_src,
		},
	)?;

	process_component(ctx, Component(button), widget.id, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
