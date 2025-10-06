use crate::{
	components::{Component, button},
	drawing::Color,
	i18n::Translation,
	layout::WidgetID,
	parser::{
		AttribPair, ParserContext, ParserFile, parse_check_f32, parse_children, process_component,
		style::{parse_color_opt, parse_round, parse_style, parse_text_style},
	},
	widget::util::WLength,
};

pub fn parse_component_button<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut color: Option<Color> = None;
	let mut border = 2.0;
	let mut border_color: Option<Color> = None;
	let mut hover_color: Option<Color> = None;
	let mut hover_border_color: Option<Color> = None;
	let mut round = WLength::Units(4.0);

	let mut translation: Option<Translation> = None;

	let text_style = parse_text_style(attribs);
	let style = parse_style(attribs);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"text" => {
				translation = Some(Translation::from_raw_text(value));
			}
			"translation" => {
				translation = Some(Translation::from_translation_key(value));
			}
			"round" => {
				parse_round(value, &mut round);
			}
			"color" => {
				parse_color_opt(value, &mut color);
			}
			"border" => {
				parse_check_f32(value, &mut border);
			}
			"border_color" => {
				parse_color_opt(value, &mut border_color);
			}
			"hover_color" => {
				parse_color_opt(value, &mut hover_color);
			}
			"hover_border_color" => {
				parse_color_opt(value, &mut hover_border_color);
			}
			_ => {}
		}
	}

	let globals = ctx.layout.state.globals.clone();

	let (widget, component) = button::construct(
		&mut globals.get(),
		ctx.layout,
		ctx.listeners,
		parent_id,
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
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
