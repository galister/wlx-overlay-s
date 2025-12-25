use wgui::{
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	frontend::{FrontendTasks, RcFrontend},
	util::various::AsyncExecutor,
};

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
	pub frontend_tasks: &'a FrontendTasks,
}

pub struct TabUpdateParams<'a> {
	pub layout: &'a mut Layout,
	pub interface: &'a mut BoxDashInterface,
	pub executor: &'a mut AsyncExecutor,
}

pub trait Tab {
	#[allow(dead_code)]
	fn get_type(&self) -> TabType;

	fn update(&mut self, _params: TabUpdateParams) -> anyhow::Result<()> {
		Ok(())
	}
}
