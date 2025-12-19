use crate::{
	components::{Component, slider},
	layout::WidgetID,
	parser::{AttribPair, ParserContext, parse_check_f32, parse_check_i32, process_component, style::parse_style},
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
	let mut show_value = 1;

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
			"show_value" => {
				parse_check_i32(value, &mut show_value);
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
			show_value: show_value != 0,
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
