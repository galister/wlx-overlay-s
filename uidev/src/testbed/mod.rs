use wgui::layout::{Layout, LayoutUpdateResult};
use wlx_common::audio;

pub mod testbed_any;
pub mod testbed_dashboard;
pub mod testbed_generic;

pub struct TestbedUpdateParams<'a> {
	pub width: f32,
	pub height: f32,
	pub timestep_alpha: f32,
	pub audio_system: &'a mut audio::AudioSystem,
	pub audio_sample_player: &'a mut audio::SamplePlayer,
}

impl<'a> TestbedUpdateParams<'a> {
	pub fn process_layout_result(&mut self, res: LayoutUpdateResult) {
		self
			.audio_sample_player
			.play_wgui_samples(self.audio_system, res.sounds_to_play);
	}
}

pub trait Testbed {
	fn update(&mut self, params: TestbedUpdateParams) -> anyhow::Result<()>;
	fn layout(&mut self) -> &mut Layout;
}
