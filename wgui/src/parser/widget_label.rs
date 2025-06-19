use crate::{
	layout::WidgetID,
	parser::{
		ParserContext, ParserFile, iter_attribs, parse_children, parse_color_hex, parse_universal,
		print_invalid_attrib, style::style_from_node,
	},
	renderer_vk::text::{FontWeight, HorizontalAlign},
	widget::text::{TextLabel, TextParams},
};

pub fn parse_widget_label<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = TextParams::default();
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();
	for (key, value) in attribs {
		match &*key {
			"text" => {
				params.content = String::from(value.as_ref());
			}
			"color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.style.color = Some(color);
				}
			}
			"align" => match &*value {
				"left" => params.style.align = Some(HorizontalAlign::Left),
				"right" => params.style.align = Some(HorizontalAlign::Right),
				"center" => params.style.align = Some(HorizontalAlign::Center),
				"justified" => params.style.align = Some(HorizontalAlign::Justified),
				"end" => params.style.align = Some(HorizontalAlign::End),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"weight" => match &*value {
				"normal" => params.style.weight = Some(FontWeight::Normal),
				"bold" => params.style.weight = Some(FontWeight::Bold),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"size" => {
				if let Ok(size) = value.parse::<f32>() {
					params.style.size = Some(size);
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			_ => {}
		}
	}

	let style = style_from_node(file, ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, TextLabel::create(params)?, style)?;

	parse_universal(file, ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}
