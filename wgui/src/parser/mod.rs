mod component_button;
mod component_checkbox;
mod component_slider;
mod style;
mod widget_div;
mod widget_label;
mod widget_rectangle;
mod widget_sprite;

use crate::{
	assets::AssetProvider,
	components::{Component, ComponentTrait, ComponentWeak},
	drawing::{self},
	event::EventListenerCollection,
	globals::WguiGlobals,
	layout::{Layout, LayoutParams, LayoutState, Widget, WidgetID, WidgetMap, WidgetPair},
	parser::{
		component_button::parse_component_button, component_checkbox::parse_component_checkbox,
		component_slider::parse_component_slider, widget_div::parse_widget_div, widget_label::parse_widget_label,
		widget_rectangle::parse_widget_rectangle, widget_sprite::parse_widget_sprite,
	},
};
use ouroboros::self_referencing;
use smallvec::SmallVec;
use std::{
	cell::RefMut,
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

/*
	WARNING: this struct could contain valid components with already bound listener handles.
	Make sure to store them somewhere in your code.
*/
#[derive(Default)]
pub struct ParserState {
	pub ids: HashMap<Rc<str>, WidgetID>,
	macro_attribs: HashMap<Rc<str>, MacroAttribs>,
	pub var_map: HashMap<Rc<str>, Rc<str>>,
	pub components: Vec<Component>,
	pub components_by_id: HashMap<Rc<str>, std::rc::Weak<dyn ComponentTrait>>,
	pub components_by_widget_id: HashMap<WidgetID, std::rc::Weak<dyn ComponentTrait>>,
	pub templates: HashMap<Rc<str>, Rc<Template>>,
	pub path: PathBuf,
}

impl ParserState {
	pub fn fetch_component_by_id(&self, id: &str) -> anyhow::Result<Component> {
		let Some(weak) = self.components_by_id.get(id) else {
			anyhow::bail!("Component by ID \"{id}\" doesn't exist");
		};

		let Some(component) = weak.upgrade() else {
			anyhow::bail!("Component by ID \"{id}\" doesn't exist");
		};

		Ok(Component(component))
	}

	pub fn fetch_component_by_widget_id(&self, widget_id: WidgetID) -> anyhow::Result<Component> {
		let Some(weak) = self.components_by_widget_id.get(&widget_id) else {
			anyhow::bail!("Component by widget ID \"{widget_id:?}\" doesn't exist");
		};

		let Some(component) = weak.upgrade() else {
			anyhow::bail!("Component by widget ID \"{widget_id:?}\" doesn't exist");
		};

		Ok(Component(component))
	}

	pub fn fetch_component_as<T: 'static>(&self, id: &str) -> anyhow::Result<Rc<T>> {
		let component = self.fetch_component_by_id(id)?;

		if !(*component.0).as_any().is::<T>() {
			anyhow::bail!("fetch_component_as({id}): type not matching");
		}

		// safety: we already checked it above, should be safe to directly cast it
		unsafe { Ok(Rc::from_raw(Rc::into_raw(component.0).cast())) }
	}

	pub fn fetch_component_from_widget_id_as<T: 'static>(&self, widget_id: WidgetID) -> anyhow::Result<Rc<T>> {
		let component = self.fetch_component_by_widget_id(widget_id)?;

		if !(*component.0).as_any().is::<T>() {
			anyhow::bail!("fetch_component_by_widget_id({widget_id:?}): type not matching");
		}

		// safety: we already checked it above, should be safe to directly cast it
		unsafe { Ok(Rc::from_raw(Rc::into_raw(component.0).cast())) }
	}

	pub fn get_widget_id(&self, id: &str) -> anyhow::Result<WidgetID> {
		match self.ids.get(id) {
			Some(id) => Ok(*id),
			None => anyhow::bail!("Widget by ID \"{id}\" doesn't exist"),
		}
	}

	// returns widget and its id at once
	pub fn fetch_widget(&self, state: &LayoutState, id: &str) -> anyhow::Result<WidgetPair> {
		let widget_id = self.get_widget_id(id)?;
		let widget = state
			.widgets
			.get(widget_id)
			.ok_or_else(|| anyhow::anyhow!("fetch_widget({id}): widget not found"))?;
		Ok(WidgetPair {
			id: widget_id,
			widget: widget.clone(),
		})
	}

	pub fn fetch_widget_as<'a, T: 'static>(&self, state: &'a LayoutState, id: &str) -> anyhow::Result<RefMut<'a, T>> {
		let widget_id = self.get_widget_id(id)?;
		let widget = state
			.widgets
			.get(widget_id)
			.ok_or_else(|| anyhow::anyhow!("fetch_widget_as({id}): widget not found"))?;

		let casted = widget
			.get_as_mut::<T>()
			.ok_or_else(|| anyhow::anyhow!("fetch_widget_as({id}): failed to cast"))?;

		Ok(casted)
	}

	pub fn process_template<U1, U2>(
		&mut self,
		doc_params: &ParseDocumentParams,
		template_name: &str,
		layout: &mut Layout,
		listeners: &mut EventListenerCollection<U1, U2>,
		widget_id: WidgetID,
		template_parameters: HashMap<Rc<str>, Rc<str>>,
	) -> anyhow::Result<()> {
		let Some(template) = self.templates.get(template_name) else {
			anyhow::bail!("no template named \"{template_name}\" found");
		};

		let mut ctx = ParserContext {
			layout,
			listeners,
			ids: Default::default(),
			macro_attribs: self.macro_attribs.clone(),       // FIXME: prevent copying
			var_map: self.var_map.clone(),                   // FIXME: prevent copying
			components: self.components.clone(),             // FIXME: prevent copying
			components_by_id: self.components_by_id.clone(), // FIXME: prevent copying
			components_by_widget_id: self.components_by_widget_id.clone(), // FIXME: prevent copying
			templates: Default::default(),
			doc_params,
		};

		let file = ParserFile {
			document: template.node_document.clone(),
			path: self.path.clone(),
			template_parameters: template_parameters.clone(), // FIXME: prevent copying
		};

		parse_widget_other_internal(&template.clone(), template_parameters, &file, &mut ctx, widget_id)?;

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

struct ParserContext<'a, U1, U2> {
	doc_params: &'a ParseDocumentParams<'a>,
	layout: &'a mut Layout,
	listeners: &'a mut EventListenerCollection<U1, U2>,
	var_map: HashMap<Rc<str>, Rc<str>>,
	macro_attribs: HashMap<Rc<str>, MacroAttribs>,
	ids: HashMap<Rc<str>, WidgetID>,
	templates: HashMap<Rc<str>, Rc<Template>>,

	components: Vec<Component>,
	components_by_id: HashMap<Rc<str>, ComponentWeak>,
	components_by_widget_id: HashMap<WidgetID, ComponentWeak>,
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
	} else if html_hex.len() == 9
		&& let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
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
	log::warn!("failed to parse color \"{html_hex}\"");
	None
}

fn get_tag_by_name<'a>(node: &roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
	node.children().find(|&child| child.tag_name().name() == name)
}

fn require_tag_by_name<'a>(node: &roxmltree::Node<'a, 'a>, name: &str) -> anyhow::Result<roxmltree::Node<'a, 'a>> {
	get_tag_by_name(node, name).ok_or_else(|| anyhow::anyhow!("Tag \"{name}\" not found"))
}

fn print_invalid_attrib(key: &str, value: &str) {
	log::warn!("Invalid value \"{value}\" in attribute \"{key}\"");
}

fn print_missing_attrib(tag_name: &str, attr: &str) {
	log::warn!("Missing attribute {attr} in tag <{tag_name}>");
}

fn print_invalid_value(value: &str) {
	log::warn!("Invalid value \"{value}\"");
}

fn parse_val(value: &Rc<str>) -> Option<f32> {
	let Ok(val) = value.parse::<f32>() else {
		print_invalid_value(value);
		return None;
	};
	Some(val)
}

fn is_percent(value: &str) -> bool {
	value.ends_with('%')
}

fn parse_percent(value: &str) -> Option<f32> {
	let Some(val_str) = value.split('%').next() else {
		print_invalid_value(value);
		return None;
	};

	let Ok(val) = val_str.parse::<f32>() else {
		print_invalid_value(value);
		return None;
	};
	Some(val / 100.0)
}

fn parse_i32(value: &str) -> Option<i32> {
	value.parse::<i32>().ok()
}

fn parse_f32(value: &str) -> Option<f32> {
	value.parse::<f32>().ok()
}

fn parse_check_i32(value: &str, num: &mut i32) -> bool {
	if let Some(value) = parse_i32(value) {
		*num = value;
		true
	} else {
		print_invalid_value(value);
		false
	}
}

fn parse_check_f32(value: &str, num: &mut f32) -> bool {
	if let Some(value) = parse_f32(value) {
		*num = value;
		true
	} else {
		print_invalid_value(value);
		false
	}
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

fn parse_widget_other_internal<U1, U2>(
	template: &Rc<Template>,
	template_parameters: HashMap<Rc<str>, Rc<str>>,
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
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
		.ok_or_else(|| anyhow::anyhow!("template node invalid"))?;

	parse_children(&template_file, ctx, template_node, parent_id)?;

	Ok(())
}

fn parse_widget_other<'a, U1, U2>(
	xml_tag_name: &str,
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	let Some(template) = ctx.templates.get(xml_tag_name) else {
		log::error!("Undefined tag named \"{xml_tag_name}\"");
		return Ok(()); // not critical
	};

	let template_parameters: HashMap<Rc<str>, Rc<str>> = iter_attribs(file, ctx, &node, false).collect();

	parse_widget_other_internal(&template.clone(), template_parameters, file, ctx, parent_id)
}

fn parse_tag_include<'a, U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	for attrib in node.attributes() {
		let (key, value) = (attrib.name(), attrib.value());

		#[allow(clippy::single_match)]
		match key {
			"src" => {
				let mut new_path = file.path.parent().unwrap_or_else(|| Path::new("/")).to_path_buf();
				new_path.push(value);

				let (new_file, node_layout) = get_doc_from_path(ctx, &new_path)?;
				parse_document_root(&new_file, ctx, parent_id, node_layout)?;

				return Ok(());
			}
			_ => {
				print_invalid_attrib(key, value);
			}
		}
	}

	Ok(())
}

fn parse_tag_var<'a, U1, U2>(ctx: &mut ParserContext<U1, U2>, node: roxmltree::Node<'a, 'a>) {
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
		return;
	};

	let Some(value) = out_value else {
		print_missing_attrib("var", "value");
		return;
	};

	ctx.var_map.insert(Rc::from(key), Rc::from(value));
}

pub fn replace_vars(input: &str, vars: &HashMap<Rc<str>, Rc<str>>) -> Rc<str> {
	let re = regex::Regex::new(r"\$\{([^}]*)\}").unwrap();

	/*if !vars.is_empty() {
		log::error!("template parameters {:?}", vars);
	}*/

	let out = re.replace_all(input, |captures: &regex::Captures| {
		let input_var = &captures[1];

		if let Some(replacement) = vars.get(input_var) {
			replacement.clone()
		} else {
			log::warn!("failed to replace var named \"{input_var}\" (not found)");
			Rc::from("")
		}
	});

	Rc::from(out)
}

#[allow(clippy::manual_strip)]
fn process_attrib<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &'a ParserContext<U1, U2>,
	key: &str,
	value: &str,
) -> (Rc<str>, Rc<str>) {
	if value.starts_with('~') {
		let name = &value[1..];

		(
			Rc::from(key),
			match ctx.var_map.get(name) {
				Some(name) => name.clone(),
				None => Rc::from("undefined"),
			},
		)
	} else {
		(Rc::from(key), replace_vars(value, &file.template_parameters))
	}
}

fn iter_attribs<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &'a ParserContext<U1, U2>,
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
				for (macro_key, macro_value) in &macro_attrib.attribs {
					res.push(process_attrib(file, ctx, macro_key, macro_value));
				}
			} else {
				log::warn!("requested macro named \"{value}\" not found!");
			}
		} else {
			res.push(process_attrib(file, ctx, key, value));
		}
	}

	res.into_iter()
}

fn parse_tag_theme<'a, U1, U2>(ctx: &mut ParserContext<U1, U2>, node: roxmltree::Node<'a, 'a>) {
	for child_node in node.children() {
		let child_name = child_node.tag_name().name();
		match child_name {
			"var" => {
				parse_tag_var(ctx, child_node);
			}
			"" => { /* ignore */ }
			_ => {
				print_invalid_value(child_name);
			}
		}
	}
}

fn parse_tag_template<U1, U2>(file: &ParserFile, ctx: &mut ParserContext<U1, U2>, node: roxmltree::Node<'_, '_>) {
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
		return;
	};

	ctx.templates.insert(
		name,
		Rc::new(Template {
			node: node.id(),
			node_document: file.document.clone(),
		}),
	);
}

fn parse_tag_macro<U1, U2>(file: &ParserFile, ctx: &mut ParserContext<U1, U2>, node: roxmltree::Node<'_, '_>) {
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
					log::warn!("macro attrib \"{key}\" already defined!");
				}
			}
		}
	}

	let Some(name) = macro_name else {
		log::error!("Template name not specified, ignoring");
		return;
	};

	ctx.macro_attribs.insert(name, MacroAttribs { attribs: macro_attribs });
}

fn process_component<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	component: Component,
	widget_id: WidgetID,
) {
	ctx.components_by_widget_id.insert(widget_id, component.weak());

	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	for (key, value) in attribs {
		#[allow(clippy::single_match)]
		match key.as_ref() {
			"id" => {
				if ctx.components_by_id.insert(value.clone(), component.weak()).is_some() {
					log::warn!("duplicate component ID \"{value}\" in the same layout file!");
				}
			}
			_ => {}
		}
	}

	ctx.components.push(component);
}

fn parse_widget_universal<'a, U1, U2>(
	file: &'a ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	node: roxmltree::Node<'a, 'a>,
	widget_id: WidgetID,
) {
	let attribs: Vec<_> = iter_attribs(file, ctx, &node, false).collect();

	for (key, value) in attribs {
		#[allow(clippy::single_match)]
		match key.as_ref() {
			"id" => {
				// Attach a specific widget to name-ID map (just like getElementById)
				if ctx.ids.insert(value.clone(), widget_id).is_some() {
					log::warn!("duplicate widget ID \"{value}\" in the same layout file!");
				}
			}
			_ => {}
		}
	}
}

fn parse_child<'a, U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	parent_node: roxmltree::Node<'a, 'a>,
	child_node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	match parent_node.attribute("ignore_in_mode") {
		Some("dev") => {
			if !ctx.doc_params.extra.dev_mode {
				return Ok(()); // do not parse
			}
		}
		Some("live") => {
			if ctx.doc_params.extra.dev_mode {
				return Ok(()); // do not parse
			}
		}
		Some(s) => print_invalid_attrib("ignore_in_mode", s),
		_ => {}
	}

	let mut new_widget_id: Option<WidgetID> = None;

	match child_node.tag_name().name() {
		"include" => {
			parse_tag_include(file, ctx, child_node, parent_id)?;
		}
		"div" => {
			new_widget_id = Some(parse_widget_div(file, ctx, child_node, parent_id)?);
		}
		"rectangle" => {
			new_widget_id = Some(parse_widget_rectangle(file, ctx, child_node, parent_id)?);
		}
		"label" => {
			new_widget_id = Some(parse_widget_label(file, ctx, child_node, parent_id)?);
		}
		"sprite" => {
			new_widget_id = Some(parse_widget_sprite(file, ctx, child_node, parent_id)?);
		}
		"Button" => {
			new_widget_id = Some(parse_component_button(file, ctx, child_node, parent_id)?);
		}
		"Slider" => {
			new_widget_id = Some(parse_component_slider(file, ctx, child_node, parent_id)?);
		}
		"CheckBox" => {
			new_widget_id = Some(parse_component_checkbox(file, ctx, child_node, parent_id)?);
		}
		"" => { /* ignore */ }
		other_tag_name => {
			parse_widget_other(other_tag_name, file, ctx, child_node, parent_id)?;
		}
	}

	// check for custom attributes (if the callback is set)
	if let Some(widget_id) = new_widget_id
		&& let Some(on_custom_attribs) = &ctx.doc_params.extra.on_custom_attribs
	{
		let mut pairs = SmallVec::<[CustomAttribPair; 4]>::new();

		for attrib in child_node.attributes() {
			let attr_name = attrib.name();
			if !attr_name.starts_with('_') || attr_name.is_empty() {
				continue;
			}

			let attr_without_prefix = &attr_name[1..]; // safe

			pairs.push(CustomAttribPair {
				attrib: attr_without_prefix,
				value: attrib.value(),
			});
		}

		if !pairs.is_empty() {
			on_custom_attribs(CustomAttribsInfo {
				widgets: &ctx.layout.state.widgets,
				parent_id,
				widget_id,
				pairs: &pairs,
			});
		}
	}

	Ok(())
}

fn parse_children<'a, U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	parent_node: roxmltree::Node<'a, 'a>,
	parent_id: WidgetID,
) -> anyhow::Result<()> {
	for child_node in parent_node.children() {
		parse_child(file, ctx, parent_node, child_node, parent_id)?;
	}

	Ok(())
}

fn create_default_context<'a, U1, U2>(
	doc_params: &'a ParseDocumentParams,
	layout: &'a mut Layout,
	listeners: &'a mut EventListenerCollection<U1, U2>,
) -> ParserContext<'a, U1, U2> {
	ParserContext {
		doc_params,
		layout,
		listeners,
		ids: Default::default(),
		var_map: Default::default(),
		templates: Default::default(),
		macro_attribs: Default::default(),
		components: Default::default(),
		components_by_id: Default::default(),
		components_by_widget_id: Default::default(),
	}
}

pub struct CustomAttribPair<'a> {
	pub attrib: &'a str, // without _ at the beginning
	pub value: &'a str,
}

pub struct CustomAttribsInfo<'a> {
	pub parent_id: WidgetID,
	pub widget_id: WidgetID,
	pub widgets: &'a WidgetMap,
	pub pairs: &'a [CustomAttribPair<'a>],
}

// helper functions
impl CustomAttribsInfo<'_> {
	pub fn get_widget(&self) -> Option<&Widget> {
		self.widgets.get(self.widget_id)
	}

	pub fn get_widget_as<T: 'static>(&self) -> Option<RefMut<'_, T>> {
		self.widgets.get(self.widget_id)?.get_as_mut::<T>()
	}

	pub fn get_value(&self, attrib_name: &str) -> Option<&str> {
		// O(n) search, these pairs won't be problematically big anyways
		for pair in self.pairs {
			if pair.attrib == attrib_name {
				return Some(pair.value);
			}
		}

		None
	}

	pub fn to_owned(&self) -> CustomAttribsInfoOwned {
		CustomAttribsInfoOwned {
			parent_id: self.parent_id,
			widget_id: self.widget_id,
			pairs: self
				.pairs
				.iter()
				.map(|p| CustomAttribPairOwned {
					attrib: p.attrib.to_string(),
					value: p.value.to_string(),
				})
				.collect(),
		}
	}
}

pub struct CustomAttribPairOwned {
	pub attrib: String, // without _ at the beginning
	pub value: String,
}

pub struct CustomAttribsInfoOwned {
	pub parent_id: WidgetID,
	pub widget_id: WidgetID,
	pub pairs: Vec<CustomAttribPairOwned>,
}

impl CustomAttribsInfoOwned {
	pub fn get_value(&self, attrib_name: &str) -> Option<&str> {
		// O(n) search, these pairs won't be problematically big anyways
		for pair in &self.pairs {
			if pair.attrib == attrib_name {
				return Some(pair.value.as_str());
			}
		}

		None
	}
}

pub type OnCustomAttribsFunc = Box<dyn Fn(CustomAttribsInfo)>;

#[derive(Default)]
pub struct ParseDocumentExtra {
	pub on_custom_attribs: Option<OnCustomAttribsFunc>, // all attributes with '_' character prepended
	pub dev_mode: bool,
}

// filled-in by you in `new_layout_from_assets` function
pub struct ParseDocumentParams<'a> {
	pub globals: WguiGlobals,      // mandatory field
	pub path: &'a str,             // mandatory field
	pub extra: ParseDocumentExtra, // optional field, can be Default-ed
}

pub fn parse_from_assets<U1, U2>(
	doc_params: &ParseDocumentParams,
	layout: &mut Layout,
	listeners: &mut EventListenerCollection<U1, U2>,
	parent_id: WidgetID,
) -> anyhow::Result<ParserState> {
	let path = PathBuf::from(doc_params.path);

	let mut ctx = create_default_context(doc_params, layout, listeners);

	let (file, node_layout) = get_doc_from_path(&ctx, &path)?;
	parse_document_root(&file, &mut ctx, parent_id, node_layout)?;

	// move everything essential to the result
	let result = ParserState {
		ids: std::mem::take(&mut ctx.ids),
		templates: std::mem::take(&mut ctx.templates),
		macro_attribs: std::mem::take(&mut ctx.macro_attribs),
		var_map: std::mem::take(&mut ctx.var_map),
		components: std::mem::take(&mut ctx.components),
		components_by_id: std::mem::take(&mut ctx.components_by_id),
		components_by_widget_id: std::mem::take(&mut ctx.components_by_widget_id),
		path,
	};

	drop(ctx);

	Ok(result)
}

pub fn new_layout_from_assets<U1, U2>(
	listeners: &mut EventListenerCollection<U1, U2>,
	doc_params: &ParseDocumentParams,
	layout_params: &LayoutParams,
) -> anyhow::Result<(Layout, ParserState)> {
	let mut layout = Layout::new(doc_params.globals.clone(), layout_params)?;
	let widget = layout.root_widget;
	let state = parse_from_assets(doc_params, &mut layout, listeners, widget)?;
	Ok((layout, state))
}

fn assets_path_to_xml(assets: &mut Box<dyn AssetProvider>, path: &Path) -> anyhow::Result<String> {
	let data = assets.load_from_path(&path.to_string_lossy())?;
	Ok(String::from_utf8(data)?)
}

fn get_doc_from_path<U1, U2>(
	ctx: &ParserContext<U1, U2>,
	path: &Path,
) -> anyhow::Result<(ParserFile, roxmltree::NodeId)> {
	let xml = assets_path_to_xml(&mut ctx.layout.state.globals.assets(), path)?;
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

fn parse_document_root<U1, U2>(
	file: &ParserFile,
	ctx: &mut ParserContext<U1, U2>,
	parent_id: WidgetID,
	node_layout: roxmltree::NodeId,
) -> anyhow::Result<()> {
	let node_layout = file
		.document
		.borrow_doc()
		.get_node(node_layout)
		.ok_or_else(|| anyhow::anyhow!("layout node not found"))?;

	for child_node in node_layout.children() {
		#[allow(clippy::single_match)]
		match child_node.tag_name().name() {
			/*  topmost include directly in <layout>  */
			"include" => parse_tag_include(file, ctx, child_node, parent_id)?,
			"theme" => parse_tag_theme(ctx, child_node),
			"template" => parse_tag_template(file, ctx, child_node),
			"macro" => parse_tag_macro(file, ctx, child_node),
			_ => {}
		}
	}

	if let Some(tag_elements) = get_tag_by_name(&node_layout, "elements") {
		parse_children(file, ctx, tag_elements, parent_id)?;
	}

	Ok(())
}
