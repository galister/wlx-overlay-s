use crate::{
	i18n::Translation,
	layout::WidgetID,
	parser::{
		AttribPair, ParserContext, ParserFile, parse_children, parse_widget_universal,
		style::{parse_style, parse_text_style},
	},
	widget::label::{WidgetLabel, WidgetLabelParams},
};

pub fn parse_widget_label<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
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

	parse_widget_universal(ctx, widget.id, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
