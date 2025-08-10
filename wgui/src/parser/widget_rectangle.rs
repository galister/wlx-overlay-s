use crate::{
	drawing::GradientMode,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_widget_universal,
		print_invalid_attrib,
		style::{parse_color, parse_round, parse_style},
	},
	widget::{self, rectangle::WidgetRectangleParams},
};

pub fn parse_widget_rectangle<'a, U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = WidgetRectangleParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	let style = parse_style(&attribs);

	for (key, value) in attribs {
		match &*key {
			"color" => {
				parse_color(&value, &mut params.color);
			}
			"color2" => {
				parse_color(&value, &mut params.color2);
			}
			"gradient" => {
				params.gradient = match &*value {
					"horizontal" => GradientMode::Horizontal,
					"vertical" => GradientMode::Vertical,
					"radial" => GradientMode::Radial,
					"none" => GradientMode::None,
					_ => {
						print_invalid_attrib(&key, &value);
						GradientMode::None
					}
				}
			}
			"round" => {
				parse_round(&value, &mut params.round);
			}
			"border" => {
				params.border = value.parse().unwrap_or_else(|_| {
					print_invalid_attrib(&key, &value);
					0.0
				});
			}
			"border_color" => {
				parse_color(&value, &mut params.border_color);
			}
			_ => {}
		}
	}

	let (new_id, _) = ctx.layout.add_child(
		parent_id,
		widget::rectangle::WidgetRectangle::create(params)?,
		style,
	)?;

	parse_widget_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
