use crate::{
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_universal,
		style::{parse_style, parse_text_style},
	},
	widget::text::{TextLabel, TextParams},
};

pub fn parse_widget_label<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = TextParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	let style = parse_style(&attribs);
	params.style = parse_text_style(&attribs);

	for (key, value) in attribs {
		match &*key {
			"text" => {
				params.content = String::from(value.as_ref());
			}
			_ => {}
		}
	}

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, TextLabel::create(params)?, style)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
