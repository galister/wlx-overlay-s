use crate::{
	i18n::Translation,
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
				params.content = Translation::from_raw_text(&value);
			}
			"translation" => params.content = Translation::from_translation_key(&value),
			_ => {}
		}
	}

	let globals = ctx.layout.state.globals.clone();
	let mut i18n = globals.i18n();

	let (new_id, _) =
		ctx
			.layout
			.add_child(parent_id, TextLabel::create(&mut i18n, params)?, style)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
