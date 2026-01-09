use taffy::{
	AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap, JustifyContent, JustifySelf,
	Overflow,
};

use crate::{
	drawing,
	parser::{AttribPair, ParserContext, is_percent, parse_color_hex, parse_f32},
	renderer_vk::text::{FontWeight, HorizontalAlign, TextStyle},
	widget::util::WLength,
};

pub fn parse_round(
	ctx: &ParserContext<'_>,
	tag_name: &str,
	key: &str,
	value: &str,
	round: &mut WLength,
	multiplier: f32,
) {
	if is_percent(value) {
		if let Some(val) = ctx.parse_percent(tag_name, key, value) {
			*round = WLength::Percent(val);
		} else {
			ctx.print_invalid_attrib(tag_name, key, value);
		}
	} else if let Some(val) = parse_f32(value) {
		*round = WLength::Units((val * multiplier).max(0.));
	} else {
		ctx.print_invalid_attrib(tag_name, key, value);
	}
}

pub fn parse_color(ctx: &ParserContext<'_>, tag_name: &str, key: &str, value: &str, color: &mut drawing::Color) {
	if let Some(res_color) = parse_color_hex(value) {
		*color = res_color;
	} else {
		ctx.print_invalid_attrib(tag_name, key, value);
	}
}

pub fn parse_color_opt(
	ctx: &ParserContext<'_>,
	tag_name: &str,
	key: &str,
	value: &str,
	color: &mut Option<drawing::Color>,
) {
	if let Some(res_color) = parse_color_hex(value) {
		*color = Some(res_color);
	} else {
		ctx.print_invalid_attrib(tag_name, key, value);
	}
}

pub fn parse_text_style(ctx: &ParserContext<'_>, attribs: &[AttribPair], tag_name: &str) -> TextStyle {
	let mut style = TextStyle::default();

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"color" => {
				if let Some(color) = parse_color_hex(value) {
					style.color = Some(color);
				}
			}
			"align" => match value {
				"left" => style.align = Some(HorizontalAlign::Left),
				"right" => style.align = Some(HorizontalAlign::Right),
				"center" => style.align = Some(HorizontalAlign::Center),
				"justified" => style.align = Some(HorizontalAlign::Justified),
				"end" => style.align = Some(HorizontalAlign::End),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"weight" => match value {
				"light" => style.weight = Some(FontWeight::Light),
				"normal" => style.weight = Some(FontWeight::Normal),
				"bold" => style.weight = Some(FontWeight::Bold),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"size" => {
				if let Ok(size) = value.parse::<f32>() {
					style.size = Some(size);
				} else {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			}
			"shadow" => {
				if let Some(color) = parse_color_hex(value) {
					style.shadow.get_or_insert_default().color = color;
				}
			}
			"shadow_x" => {
				if let Ok(x) = value.parse::<f32>() {
					style.shadow.get_or_insert_default().x = x;
				} else {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			}
			"shadow_y" => {
				if let Ok(y) = value.parse::<f32>() {
					style.shadow.get_or_insert_default().y = y;
				} else {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			}
			"wrap" => {
				if let Ok(y) = value.parse::<i32>() {
					style.wrap = y == 1;
				} else {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			}
			_ => {}
		}
	}

	style
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub fn parse_style(ctx: &ParserContext<'_>, attribs: &[AttribPair], tag_name: &str) -> taffy::Style {
	let mut style = taffy::Style::default();

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"display" => match value {
				"flex" => style.display = Display::Flex,
				"block" => style.display = Display::Block,
				"grid" => style.display = Display::Grid,
				"none" => style.display = Display::None,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"margin_left" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.left = dim;
				}
			}
			"margin_right" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.right = dim;
				}
			}
			"margin_top" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.top = dim;
				}
			}
			"margin_bottom" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.bottom = dim;
				}
			}
			"padding_left" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.left = dim;
				}
			}
			"padding_right" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.right = dim;
				}
			}
			"padding_top" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.top = dim;
				}
			}
			"padding_bottom" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.bottom = dim;
				}
			}
			"margin" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.left = dim;
					style.margin.right = dim;
					style.margin.top = dim;
					style.margin.bottom = dim;
				}
			}
			"padding" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.left = dim;
					style.padding.right = dim;
					style.padding.top = dim;
					style.padding.bottom = dim;
				}
			}
			"overflow" => match value {
				"hidden" => {
					style.overflow.x = Overflow::Hidden;
					style.overflow.y = Overflow::Hidden;
				}
				"visible" => {
					style.overflow.x = Overflow::Visible;
					style.overflow.y = Overflow::Visible;
				}
				"clip" => {
					style.overflow.x = Overflow::Clip;
					style.overflow.y = Overflow::Clip;
				}
				"scroll" => {
					style.overflow.x = Overflow::Scroll;
					style.overflow.y = Overflow::Scroll;
				}
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"overflow_x" => match value {
				"hidden" => style.overflow.x = Overflow::Hidden,
				"visible" => style.overflow.x = Overflow::Visible,
				"clip" => style.overflow.x = Overflow::Clip,
				"scroll" => style.overflow.x = Overflow::Scroll,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"overflow_y" => match value {
				"hidden" => style.overflow.y = Overflow::Hidden,
				"visible" => style.overflow.y = Overflow::Visible,
				"clip" => style.overflow.y = Overflow::Clip,
				"scroll" => style.overflow.y = Overflow::Scroll,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"min_width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.min_size.width = dim;
				}
			}
			"min_height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.min_size.height = dim;
				}
			}
			"max_width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.max_size.width = dim;
				}
			}
			"max_height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.max_size.height = dim;
				}
			}
			"width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.size.width = dim;
				}
			}
			"height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.size.height = dim;
				}
			}
			"gap" => {
				if let Some(val) = ctx.parse_size_unit(tag_name, key, value) {
					style.gap = val;
				}
			}
			"flex_basis" => {
				if let Some(val) = ctx.parse_size_unit(tag_name, key, value) {
					style.flex_basis = val;
				}
			}
			"flex_grow" => {
				if let Some(val) = ctx.parse_val(tag_name, key, value) {
					style.flex_grow = val;
				}
			}
			"flex_shrink" => {
				if let Some(val) = ctx.parse_val(tag_name, key, value) {
					style.flex_shrink = val;
				}
			}
			"position" => match value {
				"absolute" => style.position = taffy::Position::Absolute,
				"relative" => style.position = taffy::Position::Relative,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"box_sizing" => match value {
				"border_box" => style.box_sizing = BoxSizing::BorderBox,
				"content_box" => style.box_sizing = BoxSizing::ContentBox,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_self" => match value {
				"baseline" => style.align_self = Some(AlignSelf::Baseline),
				"center" => style.align_self = Some(AlignSelf::Center),
				"end" => style.align_self = Some(AlignSelf::End),
				"flex_end" => style.align_self = Some(AlignSelf::FlexEnd),
				"flex_start" => style.align_self = Some(AlignSelf::FlexStart),
				"start" => style.align_self = Some(AlignSelf::Start),
				"stretch" => style.align_self = Some(AlignSelf::Stretch),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"justify_self" => match value {
				"center" => style.justify_self = Some(JustifySelf::Center),
				"end" => style.justify_self = Some(JustifySelf::End),
				"flex_end" => style.justify_self = Some(JustifySelf::FlexEnd),
				"flex_start" => style.justify_self = Some(JustifySelf::FlexStart),
				"start" => style.justify_self = Some(JustifySelf::Start),
				"stretch" => style.justify_self = Some(JustifySelf::Stretch),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_items" => match value {
				"baseline" => style.align_items = Some(AlignItems::Baseline),
				"center" => style.align_items = Some(AlignItems::Center),
				"end" => style.align_items = Some(AlignItems::End),
				"flex_end" => style.align_items = Some(AlignItems::FlexEnd),
				"flex_start" => style.align_items = Some(AlignItems::FlexStart),
				"start" => style.align_items = Some(AlignItems::Start),
				"stretch" => style.align_items = Some(AlignItems::Stretch),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_content" => match value {
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
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"justify_content" => match value {
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
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"flex_wrap" => match value {
				"wrap" => style.flex_wrap = FlexWrap::Wrap,
				"no_wrap" => style.flex_wrap = FlexWrap::NoWrap,
				"wrap_reverse" => style.flex_wrap = FlexWrap::WrapReverse,
				_ => {}
			},
			"flex_direction" => match value {
				"column_reverse" => style.flex_direction = FlexDirection::ColumnReverse,
				"column" => style.flex_direction = FlexDirection::Column,
				"row_reverse" => style.flex_direction = FlexDirection::RowReverse,
				"row" => style.flex_direction = FlexDirection::Row,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			_ => {}
		}
	}

	style
}
