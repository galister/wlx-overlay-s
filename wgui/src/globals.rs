use std::{
	cell::{Ref, RefCell, RefMut},
	io::Read,
	rc::Rc,
};

use crate::{
	assets::{AssetPath, AssetProvider},
	assets_internal, drawing,
	font_config::{WguiFontConfig, WguiFontSystem},
	i18n::I18n,
};

#[derive(Clone)]
pub struct Defaults {
	pub dark_mode: bool,
	pub text_color: drawing::Color,
	pub button_color: drawing::Color,
	pub accent_color: drawing::Color,
	pub danger_color: drawing::Color,
}

impl Default for Defaults {
	fn default() -> Self {
		Self {
			dark_mode: true,
			text_color: drawing::Color::new(1.0, 1.0, 1.0, 1.0),
			button_color: drawing::Color::new(1.0, 1.0, 1.0, 0.05),
			accent_color: drawing::Color::new(0.0, 0.54, 1.0, 1.0),
			danger_color: drawing::Color::new(0.8, 0.0, 0.0, 1.0),
		}
	}
}

pub struct Globals {
	pub assets_internal: Box<dyn AssetProvider>,
	pub assets_builtin: Box<dyn AssetProvider>,
	pub i18n_builtin: I18n,
	pub defaults: Defaults,
	pub font_system: WguiFontSystem,
}

#[derive(Clone)]
pub struct WguiGlobals(Rc<RefCell<Globals>>);

impl WguiGlobals {
	pub fn new(
		mut assets_builtin: Box<dyn AssetProvider>,
		defaults: Defaults,
		font_config: &WguiFontConfig,
	) -> anyhow::Result<Self> {
		let i18n_builtin = I18n::new(&mut assets_builtin)?;
		let assets_internal = Box::new(assets_internal::AssetInternal {});

		Ok(Self(Rc::new(RefCell::new(Globals {
			assets_internal,
			assets_builtin,
			i18n_builtin,
			defaults,
			font_system: WguiFontSystem::new(font_config),
		}))))
	}

	pub fn get_asset(&self, asset_path: AssetPath) -> anyhow::Result<Vec<u8>> {
		match asset_path {
			AssetPath::WguiInternal(path) => self.assets_internal().load_from_path(path),
			AssetPath::BuiltIn(path) => self.assets_builtin().load_from_path(path),
			AssetPath::Filesystem(path) => {
				let mut file = match std::fs::File::open(path) {
					Ok(f) => f,
					Err(e) => {
						anyhow::bail!("Could not open asset from {path}: {e}");
					}
				};
				/* 16 MiB safeguard */
				if file.metadata()?.len() > 16 * 1024 * 1024 {
					anyhow::bail!("Could not open asset from {path}: Over size limit (16MiB)");
				}
				let mut data = Vec::new();
				if let Err(e) = file.read_to_end(&mut data) {
					anyhow::bail!("Could not read asset from {path}: {e}");
				}
				Ok(data)
			}
		}
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
