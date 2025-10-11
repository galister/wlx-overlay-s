use crate::{
	components::{Component, checkbox},
	i18n::Translation,
	layout::WidgetID,
	parser::{AttribPair, ParserContext, parse_check_f32, parse_check_i32, process_component, style::parse_style},
	widget::ConstructEssentials,
};

pub fn parse_component_checkbox<U1, U2>(
	ctx: &mut ParserContext<U1, U2>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut box_size = 24.0;
	let mut translation = Translation::default();
	let mut checked = 0;

	let style = parse_style(attribs);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"text" => {
				translation = Translation::from_raw_text(value);
			}
			"translation" => {
				translation = Translation::from_translation_key(value);
			}
			"box_size" => {
				parse_check_f32(value, &mut box_size);
			}
			"checked" => {
				parse_check_i32(value, &mut checked);
			}
			_ => {}
		}
	}

	let (widget, component) = checkbox::construct(
		&mut ctx.get_construct_essentials(parent_id),
		checkbox::Params {
			box_size,
			text: translation,
			checked: checked != 0,
			style,
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
