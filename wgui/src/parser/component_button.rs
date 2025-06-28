use crate::{
	components::button,
	drawing::Color,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs,
		style::{parse_color, parse_round, parse_style, parse_text_style},
	},
	widget::util::WLength,
};

pub fn parse_component_button<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut color = Color::new(1.0, 1.0, 1.0, 1.0);
	let mut border_color = Color::new(0.0, 0.0, 0.0, 1.0);
	let mut round = WLength::Units(4.0);

	let mut text = String::default();

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	let text_style = parse_text_style(&attribs);
	let style = parse_style(&attribs);

	for (key, value) in attribs {
		match key.as_ref() {
			"text" => {
				text = String::from(value.as_ref());
			}
			"round" => {
				parse_round(&value, &mut round);
			}
			"color" => {
				parse_color(&value, &mut color);
			}
			"border_color" => {
				parse_color(&value, &mut border_color);
			}
			_ => {}
		}
	}

	let button = button::construct(
		ctx.layout,
		ctx.listeners,
		parent_id,
		button::Params {
			color,
			border_color,
			text: &text,
			style,
			text_style,
			round,
		},
	)?;

	Ok(())
}
