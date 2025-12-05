use std::{collections::HashMap, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::{
		button::{ButtonClickCallback, ComponentButton},
		checkbox::ComponentCheckbox,
		slider::ComponentSlider,
	},
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
};

use crate::{task::Tasks, util::pactl_wrapper};

#[derive(Clone)]
enum CurrentMode {
	Sinks,
	Sources,
	Cards,
}

#[derive(Clone)]
struct IndexAndVolume {
	idx: u32,
	volume: f32,
}

#[derive(Clone)]
enum ViewTask {
	Remount,
	SetMode(CurrentMode),
	SetSinkVolume(IndexAndVolume),
	SetSourceVolume(IndexAndVolume),
}

type ViewTasks = Tasks<ViewTask>;

pub struct View {
	tasks: ViewTasks,
	on_update: Rc<dyn Fn()>,

	globals: WguiGlobals,

	#[allow(dead_code)]
	state: ParserState,

	//entry: DesktopEntry,
	mode: CurrentMode,

	id_devices: WidgetID,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub on_update: Rc<dyn Fn()>,
}

struct ProfileDisplayName {
	name: String,
	icon_path: &'static str,
	is_vr: bool,
}

fn get_card_from_sink<'a>(
	sink: &pactl_wrapper::Sink,
	cards: &'a [pactl_wrapper::Card],
) -> Option<&'a pactl_wrapper::Card> {
	let Some(sink_dev_name) = &sink.properties.get("device.name") else {
		return None;
	};

	cards.iter().find(|&card| **sink_dev_name == card.name).map(|v| v as _)
}

fn get_card_from_source<'a>(
	source: &pactl_wrapper::Source,
	cards: &'a [pactl_wrapper::Card],
) -> Option<&'a pactl_wrapper::Card> {
	let Some(source_dev_name) = &source.properties.device_name else {
		return None;
	};

	cards
		.iter()
		.find(|&card| **source_dev_name == card.name)
		.map(|v| v as _)
}

fn does_string_mention_hmd_sink(input: &str) -> bool {
	let lwr = input.to_lowercase();
	lwr.contains("hmd") || // generic hmd name detected
	lwr.contains("index") || // Valve hardware
	lwr.contains("oculus") || // Oculus
	lwr.contains("rift") || // Also Oculus
	lwr.contains("beyond") // Bigscreen Beyond
}

fn does_string_mention_hmd_source(input: &str) -> bool {
	let lwr = input.to_lowercase();
	lwr.contains("hmd") || // generic hmd name detected
	lwr.contains("valve") || // Valve hardware
	lwr.contains("oculus") || // Oculus
	lwr.contains("beyond") // Bigscreen Beyond
}

fn is_card_mentioning_hmd(card: &pactl_wrapper::Card) -> bool {
	does_string_mention_hmd_sink(&card.properties.device_name)
}

fn is_source_mentioning_hmd(source: &pactl_wrapper::Source) -> bool {
	if let Some(source_card_name) = &source.properties.card_name
		&& does_string_mention_hmd_source(source_card_name)
	{
		return true;
	}

	// WiVRn
	if source.name == "wivrn.source" {
		return true;
	}

	false
}

fn get_profile_display_name(profile_name: &str, card: &pactl_wrapper::Card) -> ProfileDisplayName {
	let Some(profile) = card.profiles.get(profile_name) else {
		// fallback
		return ProfileDisplayName {
			name: profile_name.into(),
			icon_path: "dashboard/binary.svg",
			is_vr: false,
		};
	};

	let mut out_icon_path: &'static str;
	let mut is_vr = false;

	let prof = profile_name.to_lowercase();
	if prof.contains("analog") {
		out_icon_path = "dashboard/minijack.svg";
	} else if prof.contains("iec" /* digital */) {
		out_icon_path = "dashboard/binary.svg";
	} else if prof.contains("hdmi") {
		out_icon_path = "dashboard/displayport.svg";
	} else if prof.contains("off") {
		out_icon_path = "dashboard/sleep.svg";
	} else if prof.contains("input") {
		out_icon_path = "dashboard/microphone.svg";
	} else {
		out_icon_path = "dashboard/volume.svg"; // Default fallback
	}

	// All ports are tied to this VR headset, assign all of them to the VR icon
	if is_card_mentioning_hmd(card) {
		if prof.contains("mic") {
			// Probably microphone
			out_icon_path = "dashboard/microphone.svg";
		} else {
			out_icon_path = "dashboard/vr.svg";
		}
	}

	let mut out_name: Option<String> = None;

	for port in card.ports.values() {
		// Find profile
		for port_profile in &port.profiles {
			if !port_profile.contains("stereo") {
				continue; // we only want stereo, not surround or other types
			}

			if port_profile != profile_name {
				continue;
			}

			// Exact match! Use its device name
			let Some(product_name) = port.properties.get("device.product.name") else {
				continue;
			};

			out_name = Some(product_name.clone());

			if does_string_mention_hmd_sink(product_name) {
				// VR icon
				out_icon_path = "dashboard/vr.svg";
				is_vr = true;
			} else {
				// Monitor icon
				out_icon_path = "dashboard/displayport.svg";
			}

			break;
		}
	}

	ProfileDisplayName {
		name: if let Some(name) = out_name {
			name
		} else {
			profile.description.clone()
		},
		icon_path: out_icon_path,
		is_vr,
	}
}

fn doc_params(globals: &WguiGlobals) -> ParseDocumentParams<'_> {
	ParseDocumentParams {
		globals: globals.clone(),
		path: AssetPath::BuiltIn("gui/view/audio_settings.xml"),
		extra: Default::default(),
	}
}

trait DeviceControl {
	fn on_volume_request(&self) -> anyhow::Result<f32>;
	fn on_check(&self) -> anyhow::Result<()>;
	fn on_mute_toggle(&self) -> anyhow::Result<()>;
	fn on_volume_change(&self, volume: f32) -> anyhow::Result<()>;
}

struct ControlSink {
	tasks: ViewTasks,
	on_update: Rc<dyn Fn()>,
	sink: pactl_wrapper::Sink,
}

impl ControlSink {
	fn new(tasks: ViewTasks, on_update: Rc<dyn Fn()>, sink: pactl_wrapper::Sink) -> Self {
		Self { tasks, sink, on_update }
	}
}

impl DeviceControl for ControlSink {
	fn on_volume_request(&self) -> anyhow::Result<f32> {
		let volume = pactl_wrapper::get_sink_volume(&self.sink)?;
		Ok(volume)
	}

	fn on_check(&self) -> anyhow::Result<()> {
		pactl_wrapper::set_default_sink(self.sink.index)?;
		self.tasks.push(ViewTask::Remount);
		(*self.on_update)();
		Ok(())
	}

	fn on_mute_toggle(&self) -> anyhow::Result<()> {
		pactl_wrapper::set_sink_mute(self.sink.index, !self.sink.mute)?;
		self.tasks.push(ViewTask::Remount);
		(*self.on_update)();
		Ok(())
	}

	fn on_volume_change(&self, volume: f32) -> anyhow::Result<()> {
		self.tasks.push(ViewTask::SetSinkVolume(IndexAndVolume {
			idx: self.sink.index,
			volume,
		}));
		(*self.on_update)();
		Ok(())
	}
}

struct ControlSource {
	tasks: ViewTasks,
	on_update: Rc<dyn Fn()>,
	source: pactl_wrapper::Source,
}

impl ControlSource {
	fn new(tasks: ViewTasks, on_update: Rc<dyn Fn()>, source: pactl_wrapper::Source) -> Self {
		Self {
			tasks,
			source,
			on_update,
		}
	}
}

impl DeviceControl for ControlSource {
	fn on_volume_request(&self) -> anyhow::Result<f32> {
		let volume = pactl_wrapper::get_source_volume(&self.source)?;
		Ok(volume)
	}

	fn on_check(&self) -> anyhow::Result<()> {
		pactl_wrapper::set_default_source(self.source.index)?;
		self.tasks.push(ViewTask::Remount);
		(*self.on_update)();
		Ok(())
	}

	fn on_mute_toggle(&self) -> anyhow::Result<()> {
		pactl_wrapper::set_source_mute(self.source.index, !self.source.mute)?;
		self.tasks.push(ViewTask::Remount);
		(*self.on_update)();
		Ok(())
	}

	fn on_volume_change(&self, volume: f32) -> anyhow::Result<()> {
		self.tasks.push(ViewTask::SetSourceVolume(IndexAndVolume {
			idx: self.source.index,
			volume,
		}));
		(*self.on_update)();
		Ok(())
	}
}

struct MountCardParams<'a> {
	layout: &'a mut Layout,
	card: &'a pactl_wrapper::Card,
}

struct MountDeviceSliderParams<'a> {
	layout: &'a mut Layout,
	control: Rc<dyn DeviceControl>,
	checked: bool,
	muted: bool,
	disp: Option<ProfileDisplayName>,
	alt_desc: String,
}

const ONE_HUNDRED_PERCENT: f32 = 100.0;
const VOLUME_MULT: f32 = 1.0 / ONE_HUNDRED_PERCENT;

impl View {
	fn handle_func_button_click(&self, task: ViewTask) -> ButtonClickCallback {
		let tasks = self.tasks.clone();
		let on_update = self.on_update.clone();
		Box::new(move |_common, _evt| {
			tasks.push(task.clone());
			(*on_update)();
			Ok(())
		})
	}

	pub fn new(params: Params) -> anyhow::Result<Self> {
		let tasks = ViewTasks::new();

		let state = wgui::parser::parse_from_assets(&doc_params(&params.globals), params.layout, params.parent_id)?;

		let id_devices = state.get_widget_id("devices")?;

		let btn_sinks = state.fetch_component_as::<ComponentButton>("btn_sinks")?;
		let btn_sources = state.fetch_component_as::<ComponentButton>("btn_sources")?;
		let btn_cards = state.fetch_component_as::<ComponentButton>("btn_cards")?;

		let mut res = Self {
			globals: params.globals,
			state,
			mode: CurrentMode::Sinks,
			id_devices,
			tasks,
			on_update: params.on_update,
		};

		btn_sinks.on_click(res.handle_func_button_click(ViewTask::SetMode(CurrentMode::Sinks)));
		btn_sources.on_click(res.handle_func_button_click(ViewTask::SetMode(CurrentMode::Sources)));
		btn_cards.on_click(res.handle_func_button_click(ViewTask::SetMode(CurrentMode::Cards)));

		res.init_mode_sinks(params.layout)?;

		Ok(res)
	}

	fn process_tasks(&mut self, layout: &mut Layout) -> anyhow::Result<bool> {
		let tasks = self.tasks.drain();
		if tasks.is_empty() {
			return Ok(false);
		}

		let mut set_sink_volume: Option<IndexAndVolume> = None;
		let mut set_source_volume: Option<IndexAndVolume> = None;

		for task in tasks {
			match task {
				ViewTask::Remount => match self.mode {
					CurrentMode::Sinks => self.init_mode_sinks(layout)?,
					CurrentMode::Sources => self.init_mode_sources(layout)?,
					CurrentMode::Cards => self.init_mode_cards(layout)?,
				},
				ViewTask::SetSinkVolume(s) => {
					set_sink_volume = Some(s);
				}
				ViewTask::SetSourceVolume(s) => {
					set_source_volume = Some(s);
				}
				ViewTask::SetMode(current_mode) => {
					self.mode = current_mode;
					self.tasks.push(ViewTask::Remount);
				}
			}
		}

		// set volume only to the latest event (prevent cpu time starvation
		// due to excessive input motion events)
		if let Some(s) = set_sink_volume {
			pactl_wrapper::set_sink_volume(s.idx, s.volume)?;
		}

		if let Some(s) = set_source_volume {
			pactl_wrapper::set_source_volume(s.idx, s.volume)?;
		}

		Ok(true)
	}

	pub fn update(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		while self.process_tasks(layout)? {}

		Ok(())
	}

	fn mount_card(&mut self, params: MountCardParams) -> anyhow::Result<()> {
		log::info!("mount card TODO: {}", params.card.name);
		Ok(())
	}

	fn mount_device_slider(&mut self, params: MountDeviceSliderParams) -> anyhow::Result<()> {
		let mut par = HashMap::<Rc<str>, Rc<str>>::new();

		if let Some(disp) = &params.disp {
			par.insert("device_name".into(), disp.name.as_str().into());
			par.insert("device_icon".into(), disp.icon_path.into());
		} else {
			par.insert("device_name".into(), params.alt_desc.into());
			par.insert("device_icon".into(), "dashboard/binary.svg".into());
		}

		par.insert(
			"volume_icon".into(),
			if params.muted {
				"dashboard/volume_off.svg".into()
			} else {
				"dashboard/volume.svg".into()
			},
		);

		let data = self.state.parse_template(
			&doc_params(&self.globals),
			"DeviceSlider",
			params.layout,
			self.id_devices,
			par,
		)?;

		let mut c = params.layout.start_common();
		let mut common = c.common();

		let checkbox = data.fetch_component_as::<ComponentCheckbox>("checkbox")?;
		let btn_mute = data.fetch_component_as::<ComponentButton>("btn_mute")?;
		let slider = data.fetch_component_as::<ComponentSlider>("slider")?;

		slider.set_value(&mut common, params.control.on_volume_request()? / VOLUME_MULT);

		checkbox.set_checked(&mut common, params.checked);

		checkbox.on_toggle({
			let control = params.control.clone();
			Box::new(move |_common, _event| {
				control.on_check()?;
				Ok(())
			})
		});

		slider.on_value_changed({
			let control = params.control.clone();
			Box::new(move |_common, event| {
				control.on_volume_change(event.value * VOLUME_MULT)?;
				Ok(())
			})
		});

		btn_mute.on_click({
			let control = params.control.clone();
			Box::new(move |_common, _event| {
				control.on_mute_toggle()?;
				Ok(())
			})
		});

		c.finish()?;

		Ok(())
	}

	fn init_mode_sinks(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		log::info!("refreshing sink list");

		let sinks = pactl_wrapper::list_sinks()?;
		let cards = pactl_wrapper::list_cards()?;
		let default_sink = pactl_wrapper::get_default_sink(&sinks)?;

		layout.remove_children(self.id_devices);

		for sink in sinks {
			let card = get_card_from_sink(&sink, &cards);

			let checked = if let Some(default_sink) = &default_sink {
				sink.index == default_sink.index
			} else {
				false
			};

			let alt_desc = sink.description.clone();
			let muted = sink.mute;

			let control = Rc::new(ControlSink::new(self.tasks.clone(), self.on_update.clone(), sink));

			let disp = card
				.as_ref()
				.map(|card| get_profile_display_name(&card.active_profile, card));

			self.mount_device_slider(MountDeviceSliderParams {
				checked,
				disp,
				alt_desc,
				layout,
				control,
				muted,
			})?;
		}

		Ok(())
	}

	fn init_mode_sources(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		log::info!("refreshing source list");

		let sources = pactl_wrapper::list_sources()?;
		let cards = pactl_wrapper::list_cards()?;
		let default_source = pactl_wrapper::get_default_source(&sources)?;

		layout.remove_children(self.id_devices);

		for source in sources {
			let card = get_card_from_source(&source, &cards);

			let checked = if let Some(default_source) = &default_source {
				source.index == default_source.index
			} else {
				false
			};

			let alt_desc = source.description.clone();
			let muted = source.mute;

			let control = Rc::new(ControlSource::new(self.tasks.clone(), self.on_update.clone(), source));

			let disp = card
				.as_ref()
				.map(|card| get_profile_display_name(&card.active_profile, card));

			self.mount_device_slider(MountDeviceSliderParams {
				checked,
				disp,
				alt_desc,
				layout,
				control,
				muted,
			})?;
		}

		Ok(())
	}

	fn init_mode_cards(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		log::info!("refreshing card list");

		let cards = pactl_wrapper::list_cards()?;
		layout.remove_children(self.id_devices);

		for card in cards {
			self.mount_card(MountCardParams { layout, card: &card })?;
		}

		Ok(())
	}
}
