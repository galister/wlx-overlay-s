use crate::{
	components::{Component, checkbox, radio_group::ComponentRadioGroup},
	i18n::Translation,
	layout::WidgetID,
	parser::{
		AttribPair, Fetchable, ParserContext, parse_check_f32, parse_check_i32, process_component, style::parse_style,
	},
};

pub enum CheckboxKind {
	CheckBox,
	RadioBox,
}

pub fn parse_component_checkbox(
	ctx: &mut ParserContext,
	parent_id: WidgetID,
	attribs: &[AttribPair],
	kind: CheckboxKind,
) -> anyhow::Result<WidgetID> {
	let mut box_size = 24.0;
	let mut translation = Translation::default();
	let mut checked = 0;
	let mut component_value = None;

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
			"value" => {
				component_value = Some(value.into());
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

	let mut radio_group = None;

	if matches!(kind, CheckboxKind::RadioBox) {
		let mut maybe_parent_id = Some(parent_id);

		while let Some(parent_id) = maybe_parent_id {
			if let Ok(radio) = ctx
				.data_local
				.fetch_component_from_widget_id_as::<ComponentRadioGroup>(parent_id)
			{
				radio_group = Some(radio);
				break;
			}

			maybe_parent_id = ctx.layout.get_parent(parent_id).map(|(widget_id, _)| widget_id);
		}

		if radio_group.is_none() {
			log::error!("RadioBox component without a Radio group!");
		}
	}

	let (widget, component) = checkbox::construct(
		&mut ctx.get_construct_essentials(parent_id),
		checkbox::Params {
			box_size,
			text: translation,
			checked: checked != 0,
			style,
			radio_group,
			value: component_value,
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
