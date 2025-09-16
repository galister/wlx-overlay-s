use crate::{
	i18n::Translation,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_widget_universal,
		style::{parse_style, parse_text_style},
	},
	widget::label::{WidgetLabel, WidgetLabelParams},
};

pub fn parse_widget_label<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<WidgetID> {
	let mut params = WidgetLabelParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	let style = parse_style(&attribs);
	params.style = parse_text_style(&attribs);

	for (key, value) in attribs {
		match &*key {
			"text" => {
				params.content = Translation::from_raw_text(&value);
			}
			"translation" => params.content = Translation::from_translation_key(&value),
			_ => {}
		}
	}

	let globals = ctx.layout.state.globals.clone();

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, WidgetLabel::create(&mut globals.get(), params), style)?;

	parse_widget_universal(file, ctx, node, new_id);
	parse_children(file, ctx, node, new_id)?;

	Ok(new_id)
}
