use std::{
	cell::{RefCell, RefMut},
	rc::Rc,
};

use crate::{assets::AssetProvider, i18n::I18n};

pub struct Globals {
	pub assets: Box<dyn AssetProvider>,
	pub i18n: I18n,
}

#[derive(Clone)]
pub struct WguiGlobals(Rc<RefCell<Globals>>);

impl WguiGlobals {
	pub fn new(mut assets: Box<dyn AssetProvider>) -> anyhow::Result<Self> {
		let i18n = I18n::new(&mut assets)?;

		Ok(Self(Rc::new(RefCell::new(Globals { assets, i18n }))))
	}

	pub fn get(&self) -> RefMut<Globals> {
		self.0.borrow_mut()
	}

	pub fn i18n(&self) -> RefMut<I18n> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.i18n)
	}

	pub fn assets(&self) -> RefMut<Box<dyn AssetProvider>> {
		RefMut::map(self.0.borrow_mut(), |x| &mut x.assets)
	}
}
