use wgui::{
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
};

use crate::frontend::RcFrontend;

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

pub struct TabParams<'a> {
	pub globals: &'a WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub frontend: &'a RcFrontend,
	pub settings: &'a mut crate::settings::Settings,
}

pub trait Tab {
	#[allow(dead_code)]
	fn get_type(&self) -> TabType;
}
