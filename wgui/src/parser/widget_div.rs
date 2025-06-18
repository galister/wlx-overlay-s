use crate::{
	layout::WidgetID,
	parser::{ParserContext, ParserFile, parse_children, parse_universal, style::style_from_node},
	widget,
};

pub fn parse_widget_div<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let style = style_from_node(file, ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, widget::div::Div::create()?, style)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
