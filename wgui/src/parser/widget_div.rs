use crate::{
	layout::WidgetID,
	parser::{parse_children, parse_widget_universal, style::parse_style, AttribPair, ParserContext, ParserFile},
	widget::div::WidgetDiv,
};

pub fn parse_widget_div<'a, U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let style = parse_style(attribs);

	let (new_id, _) = ctx.layout.add_child(parent_id, WidgetDiv::create(), style)?;

	parse_widget_universal(ctx, new_id, attribs);
	parse_children(file, ctx, node, new_id)?;

	Ok(new_id)
}
