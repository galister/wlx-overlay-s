use crate::frontend::Frontend;

pub mod apps;
pub mod games;
pub mod home;
pub mod monado;
pub mod processes;
pub mod settings;

#[derive(Clone, Copy, Debug)]
pub enum TabType {
	Home,
	Apps,
	Games,
	Monado,
	Processes,
	Settings,
}

pub trait Tab {
	#[allow(dead_code)]
	fn get_type(&self) -> TabType;

	fn update(&mut self, _: &mut Frontend) -> anyhow::Result<()> {
		Ok(())
	}
}
