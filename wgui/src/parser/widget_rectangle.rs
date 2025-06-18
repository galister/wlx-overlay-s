use crate::{
	drawing::GradientMode,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, is_percent, iter_attribs, parse_children, parse_color_hex,
		parse_f32, parse_percent, parse_universal, print_invalid_attrib, print_invalid_value,
		style::style_from_node,
	},
	widget::{self, rectangle::RectangleParams, util::WLength},
};

pub fn parse_widget_rectangle<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = RectangleParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node).collect();

	for (key, value) in attribs {
		match &*key {
			"color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.color = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			"color2" => {
				if let Some(color) = parse_color_hex(&value) {
					params.color2 = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
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
				if is_percent(&value) {
					if let Some(val) = parse_percent(&value) {
						params.round = WLength::Percent(val);
					} else {
						print_invalid_value(&value);
					}
				} else if let Some(val) = parse_f32(&value) {
					params.round = WLength::Units(val);
				} else {
					print_invalid_value(&value);
				}
			}
			"border" => {
				params.border = value.parse().unwrap_or_else(|_| {
					print_invalid_attrib(&key, &value);
					0.0
				});
			}
			"border_color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.border_color = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			_ => {}
		}
	}

	let style = style_from_node(file, ctx, node);

	let (new_id, _) = ctx.layout.add_child(
		parent_id,
		widget::rectangle::Rectangle::create(params)?,
		style,
	)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
