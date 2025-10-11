use crate::{
	components::{Component, slider},
	layout::WidgetID,
	parser::{AttribPair, ParserContext, parse_check_f32, process_component, style::parse_style},
	widget::ConstructEssentials,
};

pub fn parse_component_slider<U1, U2>(
	ctx: &mut ParserContext<U1, U2>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
) -> anyhow::Result<WidgetID> {
	let mut min_value = 0.0;
	let mut max_value = 1.0;
	let mut initial_value = 0.5;

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
			_ => {}
		}
	}

	let (widget, component) = slider::construct(
		ConstructEssentials {
			layout: ctx.layout,
			listeners: ctx.listeners,
			parent: parent_id,
		},
		slider::Params {
			style,
			values: slider::ValuesMinMax {
				min_value,
				max_value,
				value: initial_value,
			},
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
