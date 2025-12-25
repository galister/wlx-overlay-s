use std::{path::PathBuf, str::FromStr};
use wgui::{
	assets::{AssetPath, AssetPathOwned},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
	taffy::{self, prelude::length},
	widget::{
		label::{WidgetLabel, WidgetLabelParams},
		sprite::{WidgetSprite, WidgetSpriteParams},
	},
};

use crate::util::desktop_finder;

// the compiler wants to scream
#[allow(irrefutable_let_patterns)]
pub fn get_desktop_file_icon_path(desktop_file: &desktop_finder::DesktopFile) -> AssetPathOwned {
	/*
		FIXME: why is the compiler complaining about trailing irrefutable patterns there?!?!
		looking at the PathBuf::from_str implementation, it always returns Ok() and it's inline, maybe that's why.
	*/
	if let Some(icon) = &desktop_file.icon
		&& let Ok(path) = PathBuf::from_str(icon)
	{
		return AssetPathOwned::File(path);
	}

	AssetPathOwned::BuiltIn(PathBuf::from_str("dashboard/terminal.svg").unwrap())
}

pub fn mount_simple_label(
	globals: &WguiGlobals,
	layout: &mut Layout,
	parent_id: WidgetID,
	translation: Translation,
) -> anyhow::Result<()> {
	layout.add_child(
		parent_id,
		WidgetLabel::create(
			&mut globals.get(),
			WidgetLabelParams {
				content: translation,
				..Default::default()
			},
		),
		taffy::Style::default(),
	)?;
	Ok(())
}

pub fn mount_simple_sprite_square(
	globals: &WguiGlobals,
	layout: &mut Layout,
	parent_id: WidgetID,
	size_px: f32,
	path: AssetPath,
) -> anyhow::Result<()> {
	layout.add_child(
		parent_id,
		WidgetSprite::create(WidgetSpriteParams {
			glyph_data: Some(CustomGlyphData::new(CustomGlyphContent::from_assets(globals, path)?)),
			..Default::default()
		}),
		taffy::Style {
			size: taffy::Size {
				width: length(size_px),
				height: length(size_px),
			},
			..Default::default()
		},
	)?;

	Ok(())
}
