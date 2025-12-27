use wgui::layout::Layout;

pub mod testbed_any;
pub mod testbed_dashboard;
pub mod testbed_generic;

pub struct TestbedUpdateParams {
	pub width: f32,
	pub height: f32,
	pub timestep_alpha: f32,
}

pub trait Testbed {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()>;
	fn layout(&mut self) -> &mut Layout;
}
