use std::{rc::Rc, sync::Arc};

use crate::testbed::{Testbed, TestbedUpdateParams};
use dash_frontend::frontend::{self, FrontendUpdateParams};
use wgui::{gfx::WGfx, i18n::Locale, layout::Layout, theme::WguiTheme};
use wlx_common::{
	async_executor::AsyncExecutor, dash_interface_emulated::DashInterfaceEmulated,
	locale::WayVRLangProvider,
};

pub struct TestbedDashboard {
	executor: AsyncExecutor,
	frontend: frontend::Frontend<()>,
}

impl TestbedDashboard {
	pub fn new(gfx: Arc<WGfx>) -> anyhow::Result<Self> {
		let lang_provider = WayVRLangProvider::default();
		let locale = Locale::from_env(&lang_provider);
		let interface = DashInterfaceEmulated::new(gfx, &locale);
		let palette_name = std::env::var("PALETTE").unwrap_or_else(|_| "Default".to_string());
		let executor = wlx_common::async_executor::create_local();

		let frontend = frontend::Frontend::new(frontend::InitParams {
			interface: Box::new(interface),
			show_welcome: false,
			has_monado: true,
			lang_provider: &lang_provider,
			theme: Rc::new(WguiTheme::default()),
			color_palette: &palette_name,
			executor: executor.clone(),
		})?;
		Ok(Self { frontend, executor })
	}
}

impl Testbed for TestbedDashboard {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()> {
		let res = self.frontend.update(FrontendUpdateParams {
			data: &mut (), /* nothing */
			width: params.width,
			height: params.height,
			timestep_alpha: params.timestep_alpha,
		})?;
		self.frontend.process_update(res, params.audio)?;

		while self.executor.try_tick() {}
		Ok(())
	}

	fn layout(&mut self) -> &mut Layout {
		&mut self.frontend.layout
	}
}
