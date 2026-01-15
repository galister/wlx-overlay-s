use crate::frontend::Frontend;

pub mod apps;
pub mod games;
pub mod home;
pub mod monado;
pub mod settings;

#[derive(Clone, Copy, Debug)]
pub enum TabType {
	Home,
	Apps,
	Games,
	Monado,
	Settings,
}

pub trait Tab<T> {
	#[allow(dead_code)]
	fn get_type(&self) -> TabType;

	fn update(&mut self, _: &mut Frontend<T>, _: &mut T) -> anyhow::Result<()> {
		Ok(())
	}
}
