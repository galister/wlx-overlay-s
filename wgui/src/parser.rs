use std::{
	cell::RefCell,
	collections::HashMap,
	path::{Path, PathBuf},
	rc::Rc,
};

use ouroboros::self_referencing;
use taffy::{
	AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap, JustifyContent,
	JustifySelf, Overflow,
};

use crate::{
	assets::AssetProvider,
	drawing::{self, GradientMode},
	layout::{Layout, WidgetID},
	renderer_vk::text::{
		FontWeight, HorizontalAlign,
		custom_glyph::{CustomGlyphContent, CustomGlyphData},
	},
	widget::{
		div::Div,
		rectangle::{Rectangle, RectangleParams},
		sprite::{SpriteBox, SpriteBoxParams},
		text::{TextLabel, TextParams},
		util::WLength,
	},
};

type VarMap = HashMap<Rc<str>, Rc<str>>;

#[self_referencing]
struct XmlDocument {
	xml: String,

	#[borrows(xml)]
	#[covariant]
	doc: roxmltree::Document<'this>,
}

struct Template {
	doc: Rc<XmlDocument>,
	node: roxmltree::NodeId,
	parameters: HashMap<Rc<str>, Rc<str>>,
}

#[derive(Default)]
pub struct ParserState {
	pub ids: HashMap<Rc<str>, WidgetID>,
}

impl ParserState {
	pub fn require_by_id(&self, id: &str) -> anyhow::Result<WidgetID> {
		match self.ids.get(id) {
			Some(id) => Ok(*id),
			None => anyhow::bail!("Widget by ID \"{}\" doesn't exist", id),
		}
	}
}

struct ParserContext<'a> {
	layout: &'a mut Layout,
	var_map: VarMap,
	templates: HashMap<Rc<str>, Template>,
	state: &'a mut ParserState,
}

struct ParserFile<'a> {
	path: PathBuf,
	document: Rc<XmlDocument>,
	ctx: Rc<RefCell<ParserContext<'a>>>,
}

// Parses a color from a HTML hex string
pub fn parse_color_hex(html_hex: &str) -> Option<drawing::Color> {
	if html_hex.len() == 7 {
		if let (Ok(r), Ok(g), Ok(b)) = (
			u8::from_str_radix(&html_hex[1..3], 16),
			u8::from_str_radix(&html_hex[3..5], 16),
			u8::from_str_radix(&html_hex[5..7], 16),
		) {
			return Some(drawing::Color::new(
				f32::from(r) / 255.,
				f32::from(g) / 255.,
				f32::from(b) / 255.,
				1.,
			));
		}
	} else if html_hex.len() == 9 {
		if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
			u8::from_str_radix(&html_hex[1..3], 16),
			u8::from_str_radix(&html_hex[3..5], 16),
			u8::from_str_radix(&html_hex[5..7], 16),
			u8::from_str_radix(&html_hex[7..9], 16),
		) {
			return Some(drawing::Color::new(
				f32::from(r) / 255.,
				f32::from(g) / 255.,
				f32::from(b) / 255.,
				f32::from(a) / 255.,
			));
		}
	}
	log::warn!("failed to parse color \"{}\"", html_hex);
	None
}

fn get_tag_by_name<'a>(
	node: &roxmltree::Node<'a, 'a>,
	name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
	node
		.children()
		.find(|&child| child.tag_name().name() == name)
}

fn require_tag_by_name<'a>(
	node: &roxmltree::Node<'a, 'a>,
	name: &str,
) -> anyhow::Result<roxmltree::Node<'a, 'a>> {
	get_tag_by_name(node, name).ok_or_else(|| anyhow::anyhow!("Tag \"{}\" not found", name))
}

fn print_invalid_attrib(key: &str, value: &str) {
	log::warn!("Invalid value \"{}\" in attribute \"{}\"", value, key);
}

fn print_missing_attrib(tag_name: &str, attr: &str) {
	log::warn!("Missing attribute {} in tag <{}>", attr, tag_name);
}

fn print_invalid_value(value: &str) {
	log::warn!("Invalid value \"{}\"", value);
}

fn parse_val(value: &Rc<str>) -> Option<f32> {
	let Ok(val) = value.parse::<f32>() else {
		print_invalid_value(value);
		return None;
	};
	Some(val)
}

fn is_percent(value: &str) -> bool {
	value.ends_with("%")
}

fn parse_percent(value: &str) -> Option<f32> {
	let Some(val_str) = value.split("%").next() else {
		print_invalid_value(value);
		return None;
	};

	let Ok(val) = val_str.parse::<f32>() else {
		print_invalid_value(value);
		return None;
	};
	Some(val / 100.0)
}

fn parse_f32(value: &str) -> Option<f32> {
	value.parse::<f32>().ok()
}

fn parse_size_unit<T>(value: &str) -> Option<T>
where
	T: taffy::prelude::FromPercent + taffy::prelude::FromLength,
{
	if is_percent(value) {
		Some(taffy::prelude::percent(parse_percent(value)?))
	} else {
		Some(taffy::prelude::length(parse_f32(value)?))
	}
}

fn style_from_node<'a>(ctx: &ParserContext, node: roxmltree::Node<'a, 'a>) -> taffy::Style {
	let mut style = taffy::Style {
		..Default::default()
	};

	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();

	for (key, value) in attribs {
		match &*key {
			"display" => match &*value {
				"flex" => style.display = Display::Flex,
				"block" => style.display = Display::Block,
				"grid" => style.display = Display::Grid,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"margin_left" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.margin.left = dim;
				}
			}
			"margin_right" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.margin.right = dim;
				}
			}
			"margin_top" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.margin.top = dim;
				}
			}
			"margin_bottom" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.margin.bottom = dim;
				}
			}
			"padding_left" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.padding.left = dim;
				}
			}
			"padding_right" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.padding.right = dim;
				}
			}
			"padding_top" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.padding.top = dim;
				}
			}
			"padding_bottom" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.padding.bottom = dim;
				}
			}
			"margin" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.margin.left = dim;
					style.margin.right = dim;
					style.margin.top = dim;
					style.margin.bottom = dim;
				}
			}
			"padding" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.padding.left = dim;
					style.padding.right = dim;
					style.padding.top = dim;
					style.padding.bottom = dim;
				}
			}
			"overflow_x" => match &*value {
				"hidden" => style.overflow.x = Overflow::Hidden,
				"visible" => style.overflow.x = Overflow::Visible,
				"clip" => style.overflow.x = Overflow::Clip,
				"scroll" => style.overflow.x = Overflow::Scroll,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"overflow_y" => match &*value {
				"hidden" => style.overflow.y = Overflow::Hidden,
				"visible" => style.overflow.y = Overflow::Visible,
				"clip" => style.overflow.y = Overflow::Clip,
				"scroll" => style.overflow.y = Overflow::Scroll,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"min_width" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.min_size.width = dim;
				}
			}
			"min_height" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.min_size.height = dim;
				}
			}
			"max_width" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.max_size.width = dim;
				}
			}
			"max_height" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.max_size.height = dim;
				}
			}
			"width" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.size.width = dim;
				}
			}
			"height" => {
				if let Some(dim) = parse_size_unit(&value) {
					style.size.height = dim;
				}
			}
			"gap" => {
				if let Some(val) = parse_size_unit(&value) {
					style.gap = val;
				}
			}
			"flex_basis" => {
				if let Some(val) = parse_size_unit(&value) {
					style.flex_basis = val;
				}
			}
			"flex_grow" => {
				if let Some(val) = parse_val(&value) {
					style.flex_grow = val;
				}
			}
			"flex_shrink" => {
				if let Some(val) = parse_val(&value) {
					style.flex_shrink = val;
				}
			}
			"position" => match &*value {
				"absolute" => style.position = taffy::Position::Absolute,
				"relative" => style.position = taffy::Position::Relative,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"box_sizing" => match &*value {
				"border_box" => style.box_sizing = BoxSizing::BorderBox,
				"content_box" => style.box_sizing = BoxSizing::ContentBox,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"align_self" => match &*value {
				"baseline" => style.align_self = Some(AlignSelf::Baseline),
				"center" => style.align_self = Some(AlignSelf::Center),
				"end" => style.align_self = Some(AlignSelf::End),
				"flex_end" => style.align_self = Some(AlignSelf::FlexEnd),
				"flex_start" => style.align_self = Some(AlignSelf::FlexStart),
				"start" => style.align_self = Some(AlignSelf::Start),
				"stretch" => style.align_self = Some(AlignSelf::Stretch),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"justify_self" => match &*value {
				"center" => style.justify_self = Some(JustifySelf::Center),
				"end" => style.justify_self = Some(JustifySelf::End),
				"flex_end" => style.justify_self = Some(JustifySelf::FlexEnd),
				"flex_start" => style.justify_self = Some(JustifySelf::FlexStart),
				"start" => style.justify_self = Some(JustifySelf::Start),
				"stretch" => style.justify_self = Some(JustifySelf::Stretch),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"align_items" => match &*value {
				"baseline" => style.align_items = Some(AlignItems::Baseline),
				"center" => style.align_items = Some(AlignItems::Center),
				"end" => style.align_items = Some(AlignItems::End),
				"flex_end" => style.align_items = Some(AlignItems::FlexEnd),
				"flex_start" => style.align_items = Some(AlignItems::FlexStart),
				"start" => style.align_items = Some(AlignItems::Start),
				"stretch" => style.align_items = Some(AlignItems::Stretch),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"align_content" => match &*value {
				"center" => style.align_content = Some(AlignContent::Center),
				"end" => style.align_content = Some(AlignContent::End),
				"flex_end" => style.align_content = Some(AlignContent::FlexEnd),
				"flex_start" => style.align_content = Some(AlignContent::FlexStart),
				"space_around" => style.align_content = Some(AlignContent::SpaceAround),
				"space_between" => style.align_content = Some(AlignContent::SpaceBetween),
				"space_evenly" => style.align_content = Some(AlignContent::SpaceEvenly),
				"start" => style.align_content = Some(AlignContent::Start),
				"stretch" => style.align_content = Some(AlignContent::Stretch),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"justify_content" => match &*value {
				"center" => style.justify_content = Some(JustifyContent::Center),
				"end" => style.justify_content = Some(JustifyContent::End),
				"flex_end" => style.justify_content = Some(JustifyContent::FlexEnd),
				"flex_start" => style.justify_content = Some(JustifyContent::FlexStart),
				"space_around" => style.justify_content = Some(JustifyContent::SpaceAround),
				"space_between" => style.justify_content = Some(JustifyContent::SpaceBetween),
				"space_evenly" => style.justify_content = Some(JustifyContent::SpaceEvenly),
				"start" => style.justify_content = Some(JustifyContent::Start),
				"stretch" => style.justify_content = Some(JustifyContent::Stretch),
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			"flex_wrap" => match &*value {
				"wrap" => style.flex_wrap = FlexWrap::Wrap,
				"no_wrap" => style.flex_wrap = FlexWrap::NoWrap,
				"wrap_reverse" => style.flex_wrap = FlexWrap::WrapReverse,
				_ => {}
			},
			"flex_direction" => match &*value {
				"column_reverse" => style.flex_direction = FlexDirection::ColumnReverse,
				"column" => style.flex_direction = FlexDirection::Column,
				"row_reverse" => style.flex_direction = FlexDirection::RowReverse,
				"row" => style.flex_direction = FlexDirection::Row,
				_ => {
					print_invalid_attrib(&key, &value);
				}
			},
			_ => {}
		}
	}

	style
}

fn parse_widget_div<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let style = style_from_node(ctx, node);

	let (new_id, _) = ctx.layout.add_child(parent_id, Div::create()?, style)?;

	parse_universal(ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}

fn parse_widget_rectangle<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = RectangleParams::default();
	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();

	for (key, value) in attribs {
		match &*key {
			"color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.color = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			"color2" => {
				if let Some(color) = parse_color_hex(&value) {
					params.color2 = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			"gradient" => {
				params.gradient = match &*value {
					"horizontal" => GradientMode::Horizontal,
					"vertical" => GradientMode::Vertical,
					"radial" => GradientMode::Radial,
					"none" => GradientMode::None,
					_ => {
						print_invalid_attrib(&key, &value);
						GradientMode::None
					}
				}
			}
			"round" => {
				if is_percent(&value) {
					if let Some(val) = parse_percent(&value) {
						params.round = WLength::Percent(val);
					} else {
						print_invalid_value(&value);
					}
				} else if let Some(val) = parse_f32(&value) {
					params.round = WLength::Units(val);
				} else {
					print_invalid_value(&value);
				}
			}
			"border" => {
				params.border = value.parse().unwrap_or_else(|_| {
					print_invalid_attrib(&key, &value);
					0.0
				});
			}
			"border_color" => {
				if let Some(color) = parse_color_hex(&value) {
					params.border_color = color;
				} else {
					print_invalid_attrib(&key, &value);
				}
			}
			_ => {}
		}
	}

	let style = style_from_node(ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, Rectangle::create(params)?, style)?;

	parse_universal(ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}

fn parse_widget_sprite<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = SpriteBoxParams::default();
	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();

	let mut glyph = None;
	for (key, value) in attribs {
		match key.as_ref() {
			"src" => {
				glyph = match CustomGlyphContent::from_assets(&mut ctx.layout.assets, &value) {
					Ok(glyph) => Some(glyph),
					Err(e) => {
						log::warn!("failed to load {}: {}", value, e);
						None
					}
				}
			}
			"src_ext" => {
				if std::fs::exists(value.as_ref()).unwrap_or(false) {
					glyph = CustomGlyphContent::from_file(&value).ok();
				}
			}
			_ => {}
		}
	}

	if let Some(glyph) = glyph {
		params.glyph_data = Some(CustomGlyphData::new(glyph));
	} else {
		log::warn!("No source for sprite node!");
	};

	let style = style_from_node(ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, SpriteBox::create(params)?, style)?;

	parse_universal(ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}

fn parse_widget_label<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let mut params = TextParams::default();
	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();
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

	let style = style_from_node(ctx, node);

	let (new_id, _) = ctx
		.layout
		.add_child(parent_id, TextLabel::create(params)?, style)?;

	parse_universal(ctx, node, new_id)?;
	parse_children(file, ctx, node, new_id)?;

	Ok(())
}

fn parse_tag_include<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	for attrib in node.attributes() {
		let (key, value) = (attrib.name(), attrib.value());

		#[allow(clippy::single_match)]
		match key {
			"src" => {
				let mut new_path = file.path.parent().unwrap_or(Path::new("/")).to_path_buf();
				new_path.push(value);

				let new_file = get_doc_from_path(file.ctx.clone(), ctx, &new_path)?;
				parse_document_root(new_file, ctx, parent_id)?;

				return Ok(());
			}
			_ => {
				print_invalid_attrib(key, value);
			}
		}
	}

	Ok(())
}

fn parse_tag_var<'a>(ctx: &mut ParserContext, node: roxmltree::Node<'a, 'a>) -> anyhow::Result<()> {
	let mut out_key: Option<&str> = None;
	let mut out_value: Option<&str> = None;

	for attrib in node.attributes() {
		let (key, value) = (attrib.name(), attrib.value());

		match key {
			"key" => {
				out_key = Some(value);
			}
			"value" => {
				out_value = Some(value);
			}
			_ => {
				print_invalid_attrib(key, value);
			}
		}
	}

	let Some(key) = out_key else {
		print_missing_attrib("var", "key");
		return Ok(());
	};

	let Some(value) = out_value else {
		print_missing_attrib("var", "value");
		return Ok(());
	};

	ctx.var_map.insert(Rc::from(key), Rc::from(value));

	Ok(())
}

#[allow(clippy::manual_strip)]
pub fn iter_attribs<'a>(
	var_map: &'a VarMap,
	node: &roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = (/*key*/ Rc<str>, /*value*/ Rc<str>)> + 'a {
	node.attributes().map(|attrib| {
		let (key, value) = (attrib.name(), attrib.value());

		if value.starts_with("~") {
			let name = &value[1..];

			return (
				Rc::from(key),
				match var_map.get(name) {
					Some(name) => name.clone(),
					None => Rc::from("undefined"),
				},
			);
		}

		(Rc::from(key), Rc::from(value))
	})
}

fn parse_tag_theme<'a>(
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
) -> anyhow::Result<()> {
	for child_node in node.children() {
		let child_name = child_node.tag_name().name();
		match child_name {
			"var" => {
				parse_tag_var(ctx, child_node)?;
			}
			_ => {
				print_invalid_value(child_name);
			}
		}
	}

	Ok(())
}

fn parse_tag_template(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'_, '_>,
) -> anyhow::Result<()> {
	let mut template_name: Option<Rc<str>> = None;

	// these parameters will be passed to the children
	let mut template_parameters = HashMap::<Rc<str>, Rc<str>>::new();

	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();

	for (key, value) in attribs {
		match key.as_ref() {
			"name" => {
				template_name = Some(value);
			}
			_ => {
				template_parameters.insert(key, value);
			}
		}
	}

	let Some(name) = template_name else {
		log::error!("Template name not specified, ignoring");
		return Ok(());
	};

	ctx.templates.insert(
		name,
		Template {
			node: node.id(),
			parameters: template_parameters,
			doc: file.document.clone(),
		},
	);

	Ok(())
}

fn parse_universal<'a>(
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	widget_id: WidgetID,
) -> anyhow::Result<()> {
	let attribs: Vec<_> = iter_attribs(&ctx.var_map, &node).collect();

	for (key, value) in attribs {
		#[allow(clippy::single_match)]
		match key.as_ref() {
			"id" => {
				// Attach a specific widget to name-ID map (just like getElementById)
				if ctx.state.ids.insert(value.clone(), widget_id).is_some() {
					log::warn!("duplicate ID \"{}\" in the same layout file!", value);
				}
			}
			_ => {}
		}
	}
	Ok(())
}

fn parse_children<'a>(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	for child_node in node.children() {
		match child_node.tag_name().name() {
			"include" => {
				parse_tag_include(file, ctx, child_node, parent_id)?;
			}
			"div" => {
				parse_widget_div(file, ctx, child_node, parent_id)?;
			}
			"rectangle" => {
				parse_widget_rectangle(file, ctx, child_node, parent_id)?;
			}
			"label" => {
				parse_widget_label(file, ctx, child_node, parent_id)?;
			}
			"sprite" => {
				parse_widget_sprite(file, ctx, child_node, parent_id)?;
			}
			"" => { /* ignore */ }
			other_tag_name => {
				let Some(template) = ctx.templates.get(other_tag_name) else {
					log::error!("Undefined tag named \"{}\"", other_tag_name);
					continue;
				};

				let file = ParserFile {
					ctx: file.ctx.clone(),
					document: template.doc.clone(),
					path: file.path.clone(),
				};

				let doc = template.doc.clone();

				let template_node = doc
					.borrow_doc()
					.get_node(template.node)
					.ok_or(anyhow::anyhow!("template node invalid"))?;

				parse_children(&file, ctx, template_node, parent_id)?;
			}
		}
	}
	Ok(())
}

pub fn parse_from_assets(
	layout: &mut Layout,
	parent_id: WidgetID,
	path: &str,
) -> anyhow::Result<ParserState> {
	let path = PathBuf::from(path);
	let mut result = ParserState::default();

	let ctx_rc = Rc::new(RefCell::new(ParserContext {
		layout,
		state: &mut result,
		var_map: Default::default(),
		templates: Default::default(),
	}));

	let mut ctx = ctx_rc.borrow_mut();

	let file = get_doc_from_path(ctx_rc.clone(), &mut ctx, &path)?;
	parse_document_root(file, &mut ctx, parent_id)?;
	drop(ctx);

	Ok(result)
}

fn assets_path_to_xml(assets: &mut Box<dyn AssetProvider>, path: &Path) -> anyhow::Result<String> {
	let data = assets.load_from_path(&path.to_string_lossy())?;
	Ok(String::from_utf8(data)?)
}

fn get_doc_from_path<'a>(
	ctx_rc: Rc<RefCell<ParserContext<'a>>>,
	ctx: &mut ParserContext,
	path: &Path,
) -> anyhow::Result<ParserFile<'a>> {
	let xml = assets_path_to_xml(&mut ctx.layout.assets, path)?;
	let document = XmlDocument::new(xml, |xml| {
		let opt = roxmltree::ParsingOptions {
			allow_dtd: true,
			..Default::default()
		};
		roxmltree::Document::parse_with_options(xml, opt).unwrap()
	});

	let file = ParserFile {
		ctx: ctx_rc.clone(),
		path: PathBuf::from(path),
		document: Rc::new(document),
	};

	Ok(file)
}

fn parse_document_root(
	file: ParserFile,
	ctx: &mut ParserContext,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let root = file.document.borrow_doc().root();
	let tag_layout = require_tag_by_name(&root, "layout")?;

	for child_node in tag_layout.children() {
		#[allow(clippy::single_match)]
		match child_node.tag_name().name() {
			/*  topmost include directly in <layout>  */
			"include" => parse_tag_include(&file, ctx, child_node, parent_id)?,
			"theme" => parse_tag_theme(ctx, child_node)?,
			"template" => parse_tag_template(&file, ctx, child_node)?,
			_ => {}
		}
	}

	if let Some(tag_elements) = get_tag_by_name(&tag_layout, "elements") {
		parse_children(&file, ctx, tag_elements, parent_id)?;
	}

	Ok(())
}
