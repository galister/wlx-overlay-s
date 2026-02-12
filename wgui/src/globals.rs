use std::{
	cell::{Ref, RefCell, RefMut},
	io::Read,
	path::PathBuf,
	rc::Rc,
	sync::LazyLock,
};

use anyhow::Context;
use regex::Regex;

use crate::{
	assets::{AssetPath, AssetProvider, LangProvider},
	assets_internal, drawing,
	font_config::{WguiFontConfig, WguiFontSystem},
	i18n::I18n,
	renderer_vk::text::custom_glyph::CustomGlyphCache,
};

#[derive(Clone)]
pub struct Defaults {
	pub dark_mode: bool,
	pub text_color: drawing::Color,
	pub button_color: drawing::Color,
	pub accent_color: drawing::Color,
	pub danger_color: drawing::Color,
	pub faded_color: drawing::Color,
	pub bg_color: drawing::Color,
	pub translucent_alpha: f32,
	pub animation_mult: f32,
	pub rounding_mult: f32,
	pub gradient_intensity: f32, // currently used for buttons
}

impl Default for Defaults {
	fn default() -> Self {
		Self {
			dark_mode: true,
			text_color: drawing::Color::new(1.0, 1.0, 1.0, 1.0),
			button_color: drawing::Color::new(1.0, 1.0, 1.0, 0.02),
			accent_color: drawing::Color::new(0.13, 0.68, 1.0, 1.0),
			danger_color: drawing::Color::new(0.9, 0.0, 0.0, 1.0),
			faded_color: drawing::Color::new(0.67, 0.74, 0.80, 1.0),
			bg_color: drawing::Color::new(0.0, 0.07, 0.1, 0.75),
			translucent_alpha: 0.5,
			animation_mult: 1.0,
			rounding_mult: 1.0,
			gradient_intensity: 0.3,
		}
	}
}

pub struct Globals {
	pub assets_internal: Box<dyn AssetProvider>,
	pub assets_builtin: Box<dyn AssetProvider>,
	pub asset_folder: PathBuf,
	pub i18n_builtin: I18n,
	pub defaults: Defaults,
	pub font_system: WguiFontSystem,
	pub custom_glyph_cache: CustomGlyphCache,
}

#[derive(Clone)]
pub struct WguiGlobals(Rc<RefCell<Globals>>);

impl WguiGlobals {
	pub fn new(
		mut assets_builtin: Box<dyn AssetProvider>,
		lang_provider: &dyn LangProvider,
		defaults: Defaults,
		font_config: &WguiFontConfig,
		asset_folder: PathBuf,
	) -> anyhow::Result<Self> {
		let i18n_builtin = I18n::new(assets_builtin.as_mut(), lang_provider)?;
		let assets_internal = Box::new(assets_internal::AssetInternal {});

		Ok(Self(Rc::new(RefCell::new(Globals {
			assets_internal,
			assets_builtin,
			defaults,
			asset_folder,
			font_system: WguiFontSystem::new(font_config, i18n_builtin.get_locale()),
			i18n_builtin,
			custom_glyph_cache: CustomGlyphCache::new(),
		}))))
	}

	pub fn get_asset(&self, asset_path: AssetPath) -> anyhow::Result<Vec<u8>> {
		match asset_path {
			AssetPath::WguiInternal(path) => self.assets_internal().load_from_path(path),
			AssetPath::BuiltIn(path) => self.assets_builtin().load_from_path(path),
			AssetPath::File(path) => self.load_asset_from_fs(path),
			AssetPath::FileOrBuiltIn(path) => self
				.load_asset_from_fs(path)
				.inspect_err(|e| log::debug!("{e:?}"))
				.or_else(|_| self.assets_builtin().load_from_path(path)),
		}
	}

	fn load_asset_from_fs(&self, path: &str) -> anyhow::Result<Vec<u8>> {
		let path = expand_env_vars(path);
		let path = self.0.borrow().asset_folder.join(path);
		let mut file =
			std::fs::File::open(path.as_path()).with_context(|| format!("Could not open asset from {}", path.display()))?;

		/* 16 MiB safeguard */
		let metadata = file
			.metadata()
			.with_context(|| format!("Could not get file metadata for {}", path.display()))?;

		if metadata.len() > 16 * 1024 * 1024 {
			anyhow::bail!("Could not open asset from {}: Over size limit (16MiB)", path.display());
		}
		let mut data = Vec::new();
		file
			.read_to_end(&mut data)
			.with_context(|| format!("Could not read asset from {}", path.display()))?;
		Ok(data)
	}

	pub fn get(&self) -> RefMut<'_, Globals> {
		self.0.borrow_mut()
	}

	pub fn i18n(&self) -> RefMut<'_, I18n> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.i18n_builtin)
	}

	pub fn defaults(&self) -> Ref<'_, Defaults> {
		Ref::map(self.0.borrow(), |x| &x.defaults)
	}

	pub fn assets_internal(&self) -> RefMut<'_, Box<dyn AssetProvider>> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.assets_internal)
	}

	pub fn assets_builtin(&self) -> RefMut<'_, Box<dyn AssetProvider>> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.assets_builtin)
	}

	pub fn font_system(&self) -> RefMut<'_, WguiFontSystem> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.font_system)
	}
}

static ENV_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)}|\$([A-Z_][A-Z0-9_]*)").unwrap() // want panic
});

pub fn expand_env_vars(template: &str) -> String {
	ENV_VAR_REGEX
		.replace_all(template, |caps: &regex::Captures| {
			let var_name = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
			std::env::var(var_name)
				.inspect_err(|e| log::warn!("Unable to substitute env var {var_name}: {e:?}"))
				.unwrap_or_default()
		})
		.into_owned()
}
