use std::{
	cell::{RefCell, RefMut},
	rc::Rc,
};

use crate::{assets::AssetProvider, drawing, i18n::I18n};

pub struct Defaults {
	pub dark_mode: bool,
	pub text_color: drawing::Color,
}

impl Default for Defaults {
	fn default() -> Self {
		Self {
			dark_mode: true,
			text_color: drawing::Color::new(0.0, 0.0, 0.0, 1.0),
		}
	}
}

pub struct Globals {
	pub assets: Box<dyn AssetProvider>,
	pub i18n: I18n,
	pub defaults: Defaults,
}

#[derive(Clone)]
pub struct WguiGlobals(Rc<RefCell<Globals>>);

impl WguiGlobals {
	pub fn new(mut assets: Box<dyn AssetProvider>, defaults: Defaults) -> anyhow::Result<Self> {
		let i18n = I18n::new(&mut assets)?;

		Ok(Self(Rc::new(RefCell::new(Globals { assets, i18n, defaults }))))
	}

	pub fn get(&self) -> RefMut<'_, Globals> {
		self.0.borrow_mut()
	}

	pub fn i18n(&self) -> RefMut<'_, I18n> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.i18n)
	}

	pub fn assets(&self) -> RefMut<'_, Box<dyn AssetProvider>> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.assets)
	}
}
