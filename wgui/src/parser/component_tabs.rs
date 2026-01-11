use std::rc::Rc;

use crate::{
	assets::AssetPath,
	components::{Component, tabs},
	i18n::Translation,
	layout::WidgetID,
	parser::{AttribPair, ParserContext, ParserFile, get_asset_path_from_kv, process_component, style::parse_style},
};

pub fn parse_component_tabs<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
	attribs: &[AttribPair],
	tag_name: &str,
) -> anyhow::Result<WidgetID> {
	let style = parse_style(ctx, attribs, tag_name);

	let mut entries = Vec::<tabs::Entry>::new();

	for child in node.children() {
		match child.tag_name().name() {
			"" => { /* ignore */ }
			"Tab" => {
				let mut name: Option<&str> = None;
				let mut text: Option<Translation> = None;
				let mut sprite_src: Option<AssetPath> = None;

				for attrib in child.attributes() {
					let (key, value) = (attrib.name(), attrib.value());
					match key {
						"name" => name = Some(value),
						"text" => text = Some(Translation::from_raw_text(value)),
						"translation" => text = Some(Translation::from_translation_key(value)),
						"sprite_src" | "sprite_src_ext" | "sprite_src_builtin" | "sprite_src_internal" => {
							sprite_src = Some(get_asset_path_from_kv("sprite_", key, value));
						}
						other_key => {
							ctx.print_invalid_attrib("Tab", other_key, value);
						}
					}
				}

				if let Some(name) = name
					&& let Some(text) = text
				{
					entries.push(tabs::Entry { sprite_src, text, name });
				}
			}
			other_tag_name => {
				ctx.print_invalid_tag(tag_name, other_tag_name);
			}
		}
	}

	let (widget, component) = tabs::construct(
		&mut ctx.get_construct_essentials(parent_id),
		tabs::Params {
			style,
			selected_entry_name: "first",
			entries,
			on_select: None,
		},
	)?;

	process_component(ctx, Component(component), widget.id, attribs);

	Ok(widget.id)
}
