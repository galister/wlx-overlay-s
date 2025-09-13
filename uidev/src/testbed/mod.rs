use wgui::layout::Layout;

pub mod testbed_any;
pub mod testbed_dashboard;
pub mod testbed_generic;

pub trait Testbed {
    fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()>;
    fn layout(&mut self) -> &mut Layout;
}
