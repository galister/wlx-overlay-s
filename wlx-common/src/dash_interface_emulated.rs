use wayvr_ipc::{
	packet_client::{WvrDisplayCreateParams, WvrProcessLaunchParams},
	packet_server::{
		WvrDisplay, WvrDisplayHandle, WvrDisplayWindowLayout, WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle,
	},
};

use crate::{dash_interface::DashInterface, gen_id};

#[derive(Debug)]
pub struct EmuProcess {
	display_handle: WvrDisplayHandle,
	name: String,
}

impl EmuProcess {
	fn to(&self, handle: EmuProcessHandle) -> WvrProcess {
		WvrProcess {
			display_handle: self.display_handle.clone(),
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
pub struct EmuDisplay {
	width: u16,
	height: u16,
	name: String,
	visible: bool,
	layout: WvrDisplayWindowLayout,
}

impl EmuDisplay {
	fn to(&self, handle: EmuDisplayHandle) -> WvrDisplay {
		WvrDisplay {
			width: self.width,
			height: self.height,
			name: self.name.clone(),
			visible: self.visible,
			handle: WvrDisplayHandle {
				generation: handle.generation,
				idx: handle.idx,
			},
		}
	}
}

gen_id!(EmuDisplayVec, EmuDisplay, EmuDisplayCell, EmuDisplayHandle);
gen_id!(EmuProcessVec, EmuProcess, EmuProcessCell, EmuProcessHandle);

pub struct DashInterfaceEmulated {
	displays: EmuDisplayVec,
	processes: EmuProcessVec,
}

impl DashInterfaceEmulated {
	pub fn new() -> Self {
		Self {
			displays: EmuDisplayVec::new(),
			processes: EmuProcessVec::new(),
		}
	}
}

impl DashInterface for DashInterfaceEmulated {
	fn display_create(&mut self, params: WvrDisplayCreateParams) -> anyhow::Result<WvrDisplayHandle> {
		let res = self.displays.add(EmuDisplay {
			width: params.width,
			height: params.height,
			name: params.name,
			visible: true,
			layout: WvrDisplayWindowLayout::Tiling,
		});

		Ok(WvrDisplayHandle {
			generation: res.generation,
			idx: res.idx,
		})
	}

	fn display_get(&mut self, handle: WvrDisplayHandle) -> Option<WvrDisplay> {
		let emu_handle = EmuDisplayHandle::new(handle.idx, handle.generation);
		self.displays.get(&emu_handle).map(|disp| disp.to(emu_handle))
	}

	fn display_list(&mut self) -> anyhow::Result<Vec<WvrDisplay>> {
		Ok(self.displays.iter().map(|(handle, disp)| disp.to(handle)).collect())
	}

	fn display_remove(&mut self, handle: WvrDisplayHandle) -> anyhow::Result<()> {
		self
			.displays
			.remove(&EmuDisplayHandle::new(handle.idx, handle.generation));
		Ok(())
	}

	fn display_set_visible(&mut self, handle: WvrDisplayHandle, visible: bool) -> anyhow::Result<()> {
		let Some(disp) = self
			.displays
			.get_mut(&EmuDisplayHandle::new(handle.idx, handle.generation))
		else {
			anyhow::bail!("Display not found");
		};

		disp.visible = visible;
		Ok(())
	}

	fn display_set_window_layout(
		&mut self,
		handle: WvrDisplayHandle,
		layout: WvrDisplayWindowLayout,
	) -> anyhow::Result<()> {
		let Some(disp) = self
			.displays
			.get_mut(&EmuDisplayHandle::new(handle.idx, handle.generation))
		else {
			anyhow::bail!("Display not found");
		};

		disp.layout = layout;
		Ok(())
	}

	fn display_window_list(&mut self, _handle: WvrDisplayHandle) -> anyhow::Result<Vec<WvrWindow>> {
		// stub!
		Ok(Vec::new())
	}

	fn process_get(&mut self, handle: WvrProcessHandle) -> Option<WvrProcess> {
		let emu_handle = EmuProcessHandle::new(handle.idx, handle.generation);
		self.processes.get(&emu_handle).map(|process| process.to(emu_handle))
	}

	fn process_launch(&mut self, params: WvrProcessLaunchParams) -> anyhow::Result<WvrProcessHandle> {
		let res = self.processes.add(EmuProcess {
			display_handle: params.target_display,
			name: params.name,
		});
		Ok(WvrProcessHandle {
			generation: res.generation,
			idx: res.idx,
		})
	}

	fn process_list(&mut self) -> anyhow::Result<Vec<WvrProcess>> {
		Ok(
			self
				.processes
				.iter()
				.map(|(handle, process)| process.to(handle))
				.collect(),
		)
	}

	fn process_terminate(&mut self, handle: WvrProcessHandle) -> anyhow::Result<()> {
		self
			.processes
			.remove(&EmuProcessHandle::new(handle.idx, handle.generation));
		Ok(())
	}

	fn window_set_visible(&mut self, _handle: WvrWindowHandle, _visible: bool) -> anyhow::Result<()> {
		// stub!
		Ok(())
	}
}
