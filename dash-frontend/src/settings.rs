use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct HomeScreen {
	pub hide_username: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct General {
	pub am_pm_clock: bool,
	pub opaque_background: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Tweaks {
	pub xwayland_by_default: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
	pub home_screen: HomeScreen,
	pub general: General,
	pub tweaks: Tweaks,
}

impl Settings {
	pub fn save(&self) -> String {
		serde_json::to_string(&self).unwrap() /* want panic */
	}

	pub fn load(input: &str) -> anyhow::Result<Settings> {
		Ok(serde_json::from_str::<Settings>(input)?)
	}
}
