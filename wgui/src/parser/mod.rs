mod component_button;
mod style;
mod widget_div;
mod widget_label;
mod widget_rectangle;
mod widget_sprite;

use crate::{
	assets::AssetProvider,
	drawing::{self},
	event::EventListenerCollection,
	layout::{Layout, WidgetID},
	parser::{
		component_button::parse_component_button, widget_div::parse_widget_div,
		widget_label::parse_widget_label, widget_rectangle::parse_widget_rectangle,
		widget_sprite::parse_widget_sprite,
	},
};
use ouroboros::self_referencing;
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	rc::Rc,
};

#[self_referencing]
struct XmlDocument {
	xml: String,

	#[borrows(xml)]
	#[covariant]
	doc: roxmltree::Document<'this>,
}

pub struct Template {
	node_document: Rc<XmlDocument>,
	node: roxmltree::NodeId, // belongs to node_document which could be included in another file
}

struct ParserFile {
	path: PathBuf,
	document: Rc<XmlDocument>,
	template_parameters: HashMap<Rc<str>, Rc<str>>,
}

pub struct ParserResult {
	pub ids: HashMap<Rc<str>, WidgetID>,
	macro_attribs: HashMap<Rc<str>, MacroAttribs>,
	var_map: HashMap<Rc<str>, Rc<str>>,
	pub templates: HashMap<Rc<str>, Rc<Template>>,
	pub path: PathBuf,
}

impl ParserResult {
	pub fn require_by_id(&self, id: &str) -> anyhow::Result<WidgetID> {
		match self.ids.get(id) {
			Some(id) => Ok(*id),
			None => anyhow::bail!("Widget by ID \"{}\" doesn't exist", id),
		}
	}

	pub fn process_template(
		&mut self,
		template_name: &str,
		layout: &mut Layout,
		listeners: &mut EventListenerCollection<(), ()>,
		widget_id: WidgetID,
		template_parameters: HashMap<Rc<str>, Rc<str>>,
	) -> anyhow::Result<()> {
		let Some(template) = self.templates.get(template_name) else {
			anyhow::bail!("no template named \"{}\" found", template_name);
		};

		let mut ctx = ParserContext {
			layout,
			listeners,
			ids: Default::default(),
			macro_attribs: self.macro_attribs.clone(), // FIXME: prevent copying
			var_map: self.var_map.clone(),             // FIXME: prevent copying
			templates: Default::default(),
		};

		let file = ParserFile {
			document: template.node_document.clone(),
			path: self.path.clone(),
			template_parameters: template_parameters.clone(), // FIXME: prevent copying
		};

		parse_widget_other_internal(
			template.clone(),
			template_parameters,
			&file,
			&mut ctx,
			widget_id,
		)?;

		// FIXME?
		ctx.ids.into_iter().for_each(|(id, key)| {
			self.ids.insert(id, key);
		});

		Ok(())
	}
}

#[derive(Debug, Clone)]
struct MacroAttribs {
	attribs: HashMap<Rc<str>, Rc<str>>,
}

struct ParserContext<'a> {
	layout: &'a mut Layout,
	listeners: &'a mut EventListenerCollection<(), ()>,
	var_map: HashMap<Rc<str>, Rc<str>>,
	macro_attribs: HashMap<Rc<str>, MacroAttribs>,
	ids: HashMap<Rc<str>, WidgetID>,
	templates: HashMap<Rc<str>, Rc<Template>>,
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

fn parse_widget_other_internal(
	template: Rc<Template>,
	template_parameters: HashMap<Rc<str>, Rc<str>>,
	file: &ParserFile,
	ctx: &mut ParserContext,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let template_file = ParserFile {
		document: template.node_document.clone(),
		path: file.path.clone(),
		template_parameters,
	};

	let doc = template_file.document.clone();

	let template_node = doc
		.borrow_doc()
		.get_node(template.node)
		.ok_or(anyhow::anyhow!("template node invalid"))?;

	parse_children(&template_file, ctx, template_node, parent_id)?;

	Ok(())
}

fn parse_widget_other<'a>(
	xml_tag_name: &str,
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let Some(template) = ctx.templates.get(xml_tag_name) else {
		log::error!("Undefined tag named \"{}\"", xml_tag_name);
		return Ok(()); // not critical
	};

	let template_parameters: HashMap<Rc<str>, Rc<str>> =
		iter_attribs(file, ctx, &node, false).collect();

	parse_widget_other_internal(template.clone(), template_parameters, file, ctx, parent_id)
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

				let (new_file, node_layout) = get_doc_from_path(ctx, &new_path)?;
				parse_document_root(new_file, ctx, parent_id, node_layout)?;

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

pub fn replace_vars(input: &str, vars: &HashMap<Rc<str>, Rc<str>>) -> Rc<str> {
	let re = regex::Regex::new(r"\$\{([^}]*)\}").unwrap();

	/*if !vars.is_empty() {
		log::error!("template parameters {:?}", vars);
	}*/

	let out = re.replace_all(input, |captures: &regex::Captures| {
		let input_var = &captures[1];

		match vars.get(input_var) {
			Some(replacement) => replacement.clone(),
			None => {
				log::warn!("failed to replace var named \"{}\" (not found)", input_var);
				Rc::from("")
			}
		}
	});

	Rc::from(out)
}

#[allow(clippy::manual_strip)]
fn process_attrib<'a>(
	file: &'a ParserFile,
	ctx: &'a ParserContext,
	key: &str,
	value: &str,
) -> (Rc<str>, Rc<str>) {
	if value.starts_with("~") {
		let name = &value[1..];

		(
			Rc::from(key),
			match ctx.var_map.get(name) {
				Some(name) => name.clone(),
				None => Rc::from("undefined"),
			},
		)
	} else {
		(
			Rc::from(key),
			replace_vars(value, &file.template_parameters),
		)
	}
}

fn iter_attribs<'a>(
	file: &'a ParserFile,
	ctx: &'a ParserContext,
	node: &'a roxmltree::Node<'a, 'a>,
	is_tag_macro: bool,
) -> impl Iterator<Item = (/*key*/ Rc<str>, /*value*/ Rc<str>)> + 'a {
	let mut res = Vec::<(Rc<str>, Rc<str>)>::new();

	if is_tag_macro {
		// return as-is, no attrib post-processing
		for attrib in node.attributes() {
			let (key, value) = (attrib.name(), attrib.value());
			res.push((Rc::from(key), Rc::from(value)));
		}
		return res.into_iter();
	}

	for attrib in node.attributes() {
		let (key, value) = (attrib.name(), attrib.value());

		if key == "macro" {
			if let Some(macro_attrib) = ctx.macro_attribs.get(value) {
				for (macro_key, macro_value) in macro_attrib.attribs.iter() {
					res.push(process_attrib(file, ctx, macro_key, macro_value));
				}
			} else {
				log::warn!("requested macro named \"{}\" not found!", value);
			}
		} else {
			res.push(process_attrib(file, ctx, key, value));
		}
	}

	res.into_iter()
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
			"" => { /* ignore */ }
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

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	for (key, value) in attribs {
		match key.as_ref() {
			"name" => {
				template_name = Some(value);
			}
			_ => {
				print_invalid_attrib(&key, &value);
			}
		}
	}

	let Some(name) = template_name else {
		log::error!("Template name not specified, ignoring");
		return Ok(());
	};

	ctx.templates.insert(
		name,
		Rc::new(Template {
			node: node.id(),
			node_document: file.document.clone(),
		}),
	);

	Ok(())
}

fn parse_tag_macro(
	file: &ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'_, '_>,
) -> anyhow::Result<()> {
	let mut macro_name: Option<Rc<str>> = None;

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, true).collect();
	let mut macro_attribs = HashMap::<Rc<str>, Rc<str>>::new();

	for (key, value) in attribs {
		match key.as_ref() {
			"name" => {
				macro_name = Some(value);
			}
			_ => {
				if macro_attribs.insert(key.clone(), value).is_some() {
					log::warn!("macro attrib \"{}\" already defined!", key);
				}
			}
		}
	}

	let Some(name) = macro_name else {
		log::error!("Template name not specified, ignoring");
		return Ok(());
	};

	ctx.macro_attribs.insert(
		name.clone(),
		MacroAttribs {
			attribs: macro_attribs,
		},
	);

	Ok(())
}

fn parse_universal<'a>(
	file: &'a ParserFile,
	ctx: &mut ParserContext,
	node: roxmltree::Node<'a, 'a>,
	widget_id: WidgetID,
) -> anyhow::Result<()> {
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	for (key, value) in attribs {
		#[allow(clippy::single_match)]
		match key.as_ref() {
			"id" => {
				// Attach a specific widget to name-ID map (just like getElementById)
				if ctx.ids.insert(value.clone(), widget_id).is_some() {
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
			"button" => {
				parse_component_button(file, ctx, child_node, parent_id)?;
			}
			"" => { /* ignore */ }
			other_tag_name => {
				parse_widget_other(other_tag_name, file, ctx, child_node, parent_id)?;
			}
		}
	}
	Ok(())
}

fn create_default_context<'a>(
	layout: &'a mut Layout,
	listeners: &'a mut EventListenerCollection<(), ()>,
) -> ParserContext<'a> {
	ParserContext {
		layout,
		listeners,
		ids: Default::default(),
		var_map: Default::default(),
		templates: Default::default(),
		macro_attribs: Default::default(),
	}
}

pub fn parse_from_assets(
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<(), ()>,
	parent_id: WidgetID,
	path: &str,
) -> anyhow::Result<ParserResult> {
	let path = PathBuf::from(path);

	let mut ctx = create_default_context(layout, listeners);

	let (file, node_layout) = get_doc_from_path(&mut ctx, &path)?;
	parse_document_root(file, &mut ctx, parent_id, node_layout)?;

	// move everything essential to the result
	let result = ParserResult {
		ids: std::mem::take(&mut ctx.ids),
		templates: std::mem::take(&mut ctx.templates),
		macro_attribs: std::mem::take(&mut ctx.macro_attribs),
		var_map: std::mem::take(&mut ctx.var_map),
		path,
	};

	drop(ctx);

	Ok(result)
}

pub fn new_layout_from_assets(
	assets: Box<dyn AssetProvider>,
	listeners: &mut EventListenerCollection<(), ()>,
	path: &str,
) -> anyhow::Result<(Layout, ParserResult)> {
	let mut layout = Layout::new(assets)?;
	let widget = layout.root_widget;
	let state = parse_from_assets(&mut layout, listeners, widget, path)?;
	Ok((layout, state))
}

fn assets_path_to_xml(assets: &mut Box<dyn AssetProvider>, path: &Path) -> anyhow::Result<String> {
	let data = assets.load_from_path(&path.to_string_lossy())?;
	Ok(String::from_utf8(data)?)
}

fn get_doc_from_path(
	ctx: &mut ParserContext,
	path: &Path,
) -> anyhow::Result<(ParserFile, roxmltree::NodeId)> {
	let xml = assets_path_to_xml(&mut ctx.layout.assets, path)?;
	let document = Rc::new(XmlDocument::new(xml, |xml| {
		let opt = roxmltree::ParsingOptions {
			allow_dtd: true,
			..Default::default()
		};
		roxmltree::Document::parse_with_options(xml, opt).unwrap()
	}));

	let root = document.borrow_doc().root();
	let tag_layout = require_tag_by_name(&root, "layout")?;

	let file = ParserFile {
		path: PathBuf::from(path),
		document: document.clone(),
		template_parameters: Default::default(),
	};

	Ok((file, tag_layout.id()))
}

fn parse_document_root(
	file: ParserFile,
	ctx: &mut ParserContext,
	parent_id: WidgetID,
	node_layout: roxmltree::NodeId,
) -> anyhow::Result<()> {
	let node_layout = file
		.document
		.borrow_doc()
		.get_node(node_layout)
		.ok_or(anyhow::anyhow!("layout node not found"))?;

	for child_node in node_layout.children() {
		#[allow(clippy::single_match)]
		match child_node.tag_name().name() {
			/*  topmost include directly in <layout>  */
			"include" => parse_tag_include(&file, ctx, child_node, parent_id)?,
			"theme" => parse_tag_theme(ctx, child_node)?,
			"template" => parse_tag_template(&file, ctx, child_node)?,
			"macro" => parse_tag_macro(&file, ctx, child_node)?,
			_ => {}
		}
	}

	if let Some(tag_elements) = get_tag_by_name(&node_layout, "elements") {
		parse_children(&file, ctx, tag_elements, parent_id)?;
	}

	Ok(())
}
