use crate::{
	components::{Component, checkbox},
	i18n::Translation,
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_check_f32, parse_check_i32, parse_children,
		process_component, style::parse_style,
	},
};

pub fn parse_component_checkbox<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<WidgetID> {
	let mut box_size = 24.0;
	let mut translation = Translation::default();
	let mut checked = 0;

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	let style = parse_style(&attribs);

	for (key, value) in attribs {
		match key.as_ref() {
			"text" => {
				translation = Translation::from_raw_text(&value);
			}
			"translation" => {
				translation = Translation::from_translation_key(&value);
			}
			"box_size" => {
				parse_check_f32(value.as_ref(), &mut box_size);
			}
			"checked" => {
				parse_check_i32(value.as_ref(), &mut checked);
			}
			_ => {}
		}
	}

	let (new_id, component) = checkbox::construct(
		ctx.layout,
		ctx.listeners,
		parent_id,
		checkbox::Params {
			box_size,
			text: translation,
			checked: checked != 0,
			style,
		},
	)?;

	process_component(file, ctx, node, Component(component));

	Ok(new_id)
}
