use crate::{
	components::button,
	drawing::Color,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs,
		style::{parse_color, parse_color_opt, parse_round, parse_style, parse_text_style},
	},
	widget::util::WLength,
};

pub fn parse_component_button<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut color = Color::new(1.0, 1.0, 1.0, 1.0);
	let mut border_color: Option<Color> = None;
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
				parse_color_opt(&value, &mut border_color);
			}
			_ => {}
		}
	}

	// slight border outlines by default
	if border_color.is_none() {
		border_color = Some(Color::lerp(
			&color,
			&Color::new(0.0, 0.0, 0.0, color.a),
			0.3,
		));
	}

	let _button = button::construct(
		ctx.layout,
		ctx.listeners,
		parent_id,
		button::Params {
			color,
			border_color: border_color.unwrap(),
			text: &text,
			style,
			text_style,
			round,
		},
	)?;

	Ok(())
}
