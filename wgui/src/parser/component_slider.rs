use crate::{
	components::slider,
	layout::WidgetID,
	parser::{ParserContext, ParserFile, iter_attribs, parse_check_f32, style::parse_style},
};

pub fn parse_component_slider<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut min_value = 0.0;
	let mut max_value = 1.0;
	let mut initial_value = 0.5;

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	let style = parse_style(&attribs);

	for (key, value) in attribs {
		match key.as_ref() {
			"min_value" => {
				parse_check_f32(value.as_ref(), &mut min_value);
			}
			"max_value" => {
				parse_check_f32(value.as_ref(), &mut max_value);
			}
			"value" => {
				parse_check_f32(value.as_ref(), &mut initial_value);
			}
			_ => {}
		}
	}

	let slider = slider::construct(
		ctx.layout,
		ctx.listeners,
		parent_id,
		slider::Params {
			min_value,
			max_value,
			initial_value,
			style,
		},
	)?;

	Ok(())
}
