use crate::{
	components::{slider, Component},
	layout::WidgetID,
	parser::{parse_check_f32, process_component, style::parse_style, AttribPair, ParserContext},
	widget::ConstructEssentials,
};

pub fn parse_component_slider(
	ctx: &mut ParserContext,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut min_value = 0.0;
	let mut max_value = 1.0;
	let mut initial_value = 0.5;
	let mut step = 1.0;

	let style = parse_style(attribs);

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"min_value" => {
				parse_check_f32(value, &mut min_value);
			}
			"max_value" => {
				parse_check_f32(value, &mut max_value);
			}
			"value" => {
				parse_check_f32(value, &mut initial_value);
			}
			"step" => {
				parse_check_f32(value, &mut step);
			}
			_ => {}
		}
	}

	let (widget, component) = slider::construct(
		&mut ConstructEssentials {
			layout: ctx.layout,
			parent: parent_id,
		},
		slider::Params {
			style,
			values: slider::ValuesMinMax {
				min_value,
				max_value,
				value: initial_value,
				step,
			},
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
