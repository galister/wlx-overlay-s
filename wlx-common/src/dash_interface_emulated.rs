use wayvr_ipc::{
	packet_client::WvrProcessLaunchParams,
	packet_server::{WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle},
};

use crate::{config::GeneralConfig, dash_interface::DashInterface, desktop_finder::DesktopFinder, gen_id};

#[derive(Debug)]
pub struct EmuProcess {
	name: String,
}

impl EmuProcess {
	fn to(&self, handle: EmuProcessHandle) -> WvrProcess {
		WvrProcess {
			handle: WvrProcessHandle {
				generation: handle.generation,
				idx: handle.idx,
			},
			name: self.name.clone(),
			userdata: Default::default(),
		}
	}
}

#[derive(Debug)]
pub struct EmuWindow {
	visible: bool,
	process_handle: EmuProcessHandle,
}

impl EmuWindow {
	fn to(&self, handle: EmuWindowHandle) -> WvrWindow {
		WvrWindow {
			size_x: 1280, /* stub */
			size_y: 720,  /* stub */
			visible: true,
			handle: WvrWindowHandle {
				generation: handle.generation,
				idx: handle.idx,
			},
			process_handle: WvrProcessHandle {
				generation: self.process_handle.generation,
				idx: self.process_handle.idx,
			},
		}
	}
}

gen_id!(EmuWindowVec, EmuWindow, EmuWindowCell, EmuWindowHandle);

gen_id!(EmuProcessVec, EmuProcess, EmuProcessCell, EmuProcessHandle);

pub struct DashInterfaceEmulated {
	processes: EmuProcessVec,
	windows: EmuWindowVec,
	desktop_finder: DesktopFinder,
	general_config: GeneralConfig,
}

impl DashInterfaceEmulated {
	pub fn new() -> Self {
		let mut processes = EmuProcessVec::new();
		let process_handle = processes.add(EmuProcess {
			name: String::from("My app"),
		});

		let mut windows = EmuWindowVec::new();
		windows.add(EmuWindow {
			process_handle,
			visible: true,
		});

		let mut desktop_finder = DesktopFinder::new();
		desktop_finder.refresh();

		// Use serde defaults
		let general_config = serde_json::from_str("{}").unwrap();

		Self {
			processes,
			windows,
			desktop_finder,
			general_config,
		}
	}
}

impl Default for DashInterfaceEmulated {
	fn default() -> Self {
		Self::new()
	}
}

impl DashInterface<()> for DashInterfaceEmulated {
	fn window_list(&mut self, _: &mut ()) -> anyhow::Result<Vec<WvrWindow>> {
		Ok(self.windows.iter().map(|(handle, w)| w.to(handle)).collect())
	}

	fn window_request_close(&mut self, _: &mut (), handle: WvrWindowHandle) -> anyhow::Result<()> {
		self.windows.remove(&EmuWindowHandle {
			generation: handle.generation,
			idx: handle.idx,
		});
		Ok(())
	}

	fn process_get(&mut self, _: &mut (), handle: WvrProcessHandle) -> Option<WvrProcess> {
		let emu_handle = EmuProcessHandle::new(handle.idx, handle.generation);
		self.processes.get(&emu_handle).map(|process| process.to(emu_handle))
	}

	fn process_launch(&mut self, _: &mut (), params: WvrProcessLaunchParams) -> anyhow::Result<WvrProcessHandle> {
		let res = self.processes.add(EmuProcess { name: params.name });

		self.windows.add(EmuWindow {
			process_handle: res,
			visible: true,
		});

		Ok(WvrProcessHandle {
			generation: res.generation,
			idx: res.idx,
		})
	}

	fn process_list(&mut self, _: &mut ()) -> anyhow::Result<Vec<WvrProcess>> {
		Ok(
			self
				.processes
				.iter()
				.map(|(handle, process)| process.to(handle))
				.collect(),
		)
	}

	fn process_terminate(&mut self, _: &mut (), handle: WvrProcessHandle) -> anyhow::Result<()> {
		let mut to_remove = None;

		for (wh, w) in self.windows.iter() {
			if w.process_handle == EmuProcessHandle::new(handle.idx, handle.generation) {
				to_remove = Some(wh);
			}
		}

		if let Some(wh) = to_remove {
			self.windows.remove(&wh);
		}

		self
			.processes
			.remove(&EmuProcessHandle::new(handle.idx, handle.generation));
		Ok(())
	}

	fn window_set_visible(&mut self, _: &mut (), handle: WvrWindowHandle, visible: bool) -> anyhow::Result<()> {
		match self.windows.get_mut(&EmuWindowHandle {
			generation: handle.generation,
			idx: handle.idx,
		}) {
			Some(w) => {
				w.visible = visible;
				Ok(())
			}
			None => anyhow::bail!("Window not found"),
		}
	}

	fn recenter_playspace(&mut self, _: &mut ()) -> anyhow::Result<()> {
		// stub!
		Ok(())
	}

	fn desktop_finder<'a>(&'a mut self, _: &'a mut ()) -> &'a mut DesktopFinder {
		&mut self.desktop_finder
	}

	fn general_config<'a>(&'a mut self, _: &'a mut ()) -> &'a mut crate::config::GeneralConfig {
		&mut self.general_config
	}

	fn config_changed(&mut self, _: &mut ()) {}

	fn restart(&mut self, _data: &mut ()) {}
}
