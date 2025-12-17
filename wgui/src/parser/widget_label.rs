use crate::{
	i18n::Translation,
	layout::WidgetID,
	parser::{
		AttribPair, ParserContext, ParserFile, parse_children, parse_i32, parse_widget_universal, print_invalid_attrib,
		style::{parse_style, parse_text_style},
	},
	widget::label::{WidgetLabel, WidgetLabelParams},
};

pub fn parse_widget_label<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetLabelParams::default();

	let style = parse_style(attribs);
	params.style = parse_text_style(attribs);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"wrap" => {
				if let Some(num) = parse_i32(value) {
					params.style.wrap = num == 1;
				} else {
					print_invalid_attrib(key, value);
				}
			}
			"text" => {
				if !value.is_empty() {
					params.content = Translation::from_raw_text(value);
				}
			}
			"translation" => {
				if !value.is_empty() {
					params.content = Translation::from_translation_key(value);
				}
			}
			_ => {}
		}
	}

	let globals = ctx.layout.state.globals.clone();

	let (widget, _) = ctx
		.layout
		.add_child(parent_id, WidgetLabel::create(&mut globals.get(), params), style)?;

	parse_widget_universal(ctx, &widget, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
