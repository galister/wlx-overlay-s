use crate::{
	layout::WidgetID,
	parser::{AttribPair, ParserContext, ParserFile, parse_children, parse_widget_universal, style::parse_style},
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

	let (widget, _) = ctx.layout.add_child(parent_id, WidgetDiv::create(), style)?;

	parse_widget_universal(ctx, widget.id, attribs);
	parse_children(file, ctx, node, widget.id)?;

	Ok(widget.id)
}
