use crate::{
	components::{Component, editbox},
	layout::WidgetID,
	parser::{AttribPair, ParserContext, process_component, style::parse_style},
	widget::ConstructEssentials,
};

pub fn parse_component_editbox(
	ctx: &mut ParserContext,
	parent_id: WidgetID,
	attribs: &[AttribPair],
	tag_name: &str,
) -> anyhow::Result<WidgetID> {
	let mut initial_text = String::new();

	let style = parse_style(ctx, attribs, tag_name);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		#[allow(clippy::single_match)]
		match key {
			"text" => {
				initial_text = String::from(value);
			}
			_ => {}
		}
	}

	let (widget, component) = editbox::construct(
		&mut ConstructEssentials {
			layout: ctx.layout,
			parent: parent_id,
		},
		editbox::Params { style, initial_text },
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
