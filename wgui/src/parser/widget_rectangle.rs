use crate::{
	drawing::GradientMode,
	layout::WidgetID,
	parser::{
		parse_children, parse_widget_universal, print_invalid_attrib,
		style::{parse_color, parse_round, parse_style},
		AttribPair, ParserContext, ParserFile,
	},
	widget::rectangle::{WidgetRectangle, WidgetRectangleParams},
};

pub fn parse_widget_rectangle<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetRectangleParams::default();
	let style = parse_style(attribs);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"color" => {
				parse_color(value, &mut params.color);
			}
			"color2" => {
				parse_color(value, &mut params.color2);
			}
			"gradient" => {
				params.gradient = match value {
					"horizontal" => GradientMode::Horizontal,
					"vertical" => GradientMode::Vertical,
					"radial" => GradientMode::Radial,
					"none" => GradientMode::None,
					_ => {
						print_invalid_attrib(key, value);
						GradientMode::None
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

	let (widget, _) = ctx
		.layout
		.add_child(parent_id, WidgetRectangle::create(params), style)?;

	parse_widget_universal(ctx, &widget, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
