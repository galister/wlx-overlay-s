use std::rc::Rc;

use wayvr_ipc::packet_server::{self};
use wgui::{
	assets::AssetPath,
	components::{
		self,
		button::ComponentButton,
		tooltip::{TooltipInfo, TooltipSide},
	},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	taffy::{self, prelude::length},
	widget::{
		ConstructEssentials,
		div::WidgetDiv,
		label::{WidgetLabel, WidgetLabelParams},
	},
};
use wlx_common::dash_interface::BoxDashInterface;

use crate::{
	task::Tasks,
	util::{
		self,
		desktop_finder::{self},
		various::get_desktop_file_icon_path,
	},
};

#[derive(Clone)]
enum Task {
	Refresh,
	TerminateProcess(packet_server::WvrProcess),
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

pub struct View {
	#[allow(dead_code)]
	pub parser_state: ParserState,
	tasks: Tasks<Task>,
	globals: WguiGlobals,
	id_list_parent: WidgetID,
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/process_list.xml"),
			extra: Default::default(),
		};

		let parser_state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;
		let list_parent = parser_state.fetch_widget(&params.layout.state, "list_parent")?;

		let tasks = Tasks::new();

		tasks.push(Task::Refresh);

		Ok(Self {
			parser_state,
			tasks,
			globals: params.globals,
			id_list_parent: list_parent.id,
		})
	}

	pub fn update(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		loop {
			let tasks = self.tasks.drain();
			if tasks.is_empty() {
				break;
			}
			for task in tasks {
				match task {
					Task::Refresh => self.refresh(layout, interface)?,
					Task::TerminateProcess(process) => self.action_terminate_process(interface, process)?,
				}
			}
		}

		Ok(())
	}
}

fn get_desktop_file_from_process(
	windows: &Vec<packet_server::WvrWindow>,
	process: &packet_server::WvrProcess,
) -> Option<desktop_finder::DesktopFile> {
	for window in windows {
		if window.process_handle != process.handle {
			continue;
		}

		// TODO: refactor this after we ditch old wayvr-dashboard completely
		let Some(dfile_str) = process.userdata.get("desktop_file") else {
			continue;
		};

		let Ok(desktop_file) = serde_json::from_str::<desktop_finder::DesktopFile>(dfile_str) else {
			debug_assert!(false); // invalid json???
			continue;
		};

		return Some(desktop_file);
	}

	None
}

struct ProcessEntryResult {
	btn_terminate: Rc<ComponentButton>,
}

fn construct_process_entry(
	ess: &mut ConstructEssentials,
	globals: &WguiGlobals,
	process: &packet_server::WvrProcess,
	display: Option<&packet_server::WvrDisplay>,
	all_windows: &Vec<packet_server::WvrWindow>,
) -> anyhow::Result<ProcessEntryResult> {
	let (cell, _) = ess.layout.add_child(
		ess.parent,
		WidgetDiv::create(),
		taffy::Style {
			flex_direction: taffy::FlexDirection::Row,
			align_items: Some(taffy::AlignItems::Center),
			gap: length(8.0),
			..Default::default()
		},
	)?;

	let text_terminate_process = Translation::from_raw_text_string(globals.i18n().translate_and_replace(
		"PROCESS_LIST.TERMINATE_PROCESS_NAMED_X",
		("{PROCESS_NAME}", &process.name),
	));

	//"Terminate process" button
	let (_, btn_terminate) = components::button::construct(
		&mut ConstructEssentials {
			layout: ess.layout,
			parent: cell.id,
		},
		components::button::Params {
			sprite_src: Some(AssetPath::BuiltIn("dashboard/remove_circle.svg")),
			tooltip: Some(TooltipInfo {
				text: text_terminate_process,
				side: TooltipSide::Right,
			}),
			..Default::default()
		},
	)?;

	if let Some(desktop_file) = get_desktop_file_from_process(all_windows, process) {
		// desktop file icon and process name
		util::various::mount_simple_sprite_square(
			globals,
			ess.layout,
			cell.id,
			24.0,
			get_desktop_file_icon_path(&desktop_file).as_ref(),
		)?;

		util::various::mount_simple_label(
			globals,
			ess.layout,
			cell.id,
			Translation::from_raw_text_string(desktop_file.name.clone()),
		)?;
	} else {
		// just show a process name
		util::various::mount_simple_label(
			globals,
			ess.layout,
			cell.id,
			Translation::from_raw_text_string(process.name.clone()),
		)?;
	}

	if let Some(display) = display {
		// show display icon if available

		// "on" text
		util::various::mount_simple_label(
			globals,
			ess.layout,
			cell.id,
			Translation::from_translation_key("PROCESS_LIST.LOCATED_ON"),
		)?;

		// "display" icon
		util::various::mount_simple_sprite_square(
			globals,
			ess.layout,
			cell.id,
			24.0,
			AssetPath::BuiltIn("dashboard/display.svg"),
		)?;

		// display name itself
		util::various::mount_simple_label(
			globals,
			ess.layout,
			cell.id,
			Translation::from_raw_text_string(display.name.clone()),
		)?;
	}

	Ok(ProcessEntryResult { btn_terminate })
}

fn fill_process_list(
	globals: &WguiGlobals,
	ess: &mut ConstructEssentials,
	tasks: &Tasks<Task>,
	list: &Vec<packet_server::WvrProcess>,
	displays: &Vec<packet_server::WvrDisplay>,
	all_windows: &Vec<packet_server::WvrWindow>,
) -> anyhow::Result<()> {
	for process_entry in list {
		let mut matching_display = None;
		for display in displays {
			if process_entry.display_handle == display.handle {
				matching_display = Some(display);
			}
		}

		let entry_res = construct_process_entry(ess, globals, process_entry, matching_display, all_windows)?;

		entry_res.btn_terminate.on_click({
			let tasks = tasks.clone();
			let entry = process_entry.clone();
			Box::new(move |_, _| {
				tasks.push(Task::TerminateProcess(entry.clone()));
				Ok(())
			})
		});
	}

	Ok(())
}

impl View {
	fn refresh(&mut self, layout: &mut Layout, interface: &mut BoxDashInterface) -> anyhow::Result<()> {
		layout.remove_children(self.id_list_parent);

		let displays = interface.display_list()?;
		let all_windows = util::various::get_all_windows(interface)?;

		let mut text: Option<Translation> = None;
		match interface.process_list() {
			Ok(list) => {
				if list.is_empty() {
					text = Some(Translation::from_translation_key("PROCESS_LIST.NO_PROCESSES_FOUND"))
				} else {
					fill_process_list(
						&self.globals,
						&mut ConstructEssentials {
							layout,
							parent: self.id_list_parent,
						},
						&self.tasks,
						&list,
						&displays,
						&all_windows,
					)?;
				}
			}
			Err(e) => text = Some(Translation::from_raw_text(&format!("Error: {:?}", e))),
		}

		if let Some(text) = text.take() {
			layout.add_child(
				self.id_list_parent,
				WidgetLabel::create(
					&mut self.globals.get(),
					WidgetLabelParams {
						content: text,
						..Default::default()
					},
				),
				Default::default(),
			)?;
		}

		Ok(())
	}

	fn action_terminate_process(
		&mut self,
		interface: &mut BoxDashInterface,
		process: packet_server::WvrProcess,
	) -> anyhow::Result<()> {
		interface.process_terminate(process.handle)?;
		self.tasks.push(Task::Refresh);
		Ok(())
	}
}
