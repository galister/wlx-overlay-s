use std::rc::Rc;

use wgui::{
	globals::WguiGlobals,
	i18n::Translation,
	layout::WidgetID,
	log::LogErr,
	parser::{Fetchable, TemplateParams},
	task::Tasks,
	widget::label::WidgetLabel,
	windowing::context_menu,
};
use wlx_common::{async_executor::AsyncExecutor, config::GeneralConfig, dash_interface::ConfigChangeKind};

use crate::{
	frontend::FrontendTasks,
	tab::settings::{
		SettingType, SettingsMountParams, SettingsTab, TabNameEnum, Task as ParentTask, UpdateExtra, horiz_cell,
		macros::{MacroParams, options_category, options_checkbox, options_range_f32},
	},
	util::{
		popup_manager::PopupHolder,
		wgui_simple,
		whisper::{
			WHISPER_MODELS, WhisperModel, whisper_any_models_downloaded, whisper_delete_all_models, whisper_model_from_name,
			whisper_model_path,
		},
	},
	views::{self, ViewUpdateParams},
};

#[derive(Clone)]
enum Task {
	WhisperDownloadClosed,
	WhisperRemoveUnused,
	WhisperDownload(&'static WhisperModel),
	WhisperDownloadDone,
	CloseDialog,
	ReloadTab,
}

pub struct State {
	popup_download: PopupHolder<views::download_file::View>,
	popup_dialog: PopupHolder<views::dialog_box::View>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	tasks: Tasks<Task>,
	parent_tasks: Tasks<ParentTask>,
	pending_download: Option<&'static WhisperModel>,
}

impl SettingsTab for State {
	fn update(&mut self, par: &mut ViewUpdateParams, _extra: &UpdateExtra) -> anyhow::Result<()> {
		self.popup_download.update(par)?;
		self.popup_dialog.update(par)?;

		for task in self.tasks.drain() {
			match task {
				Task::WhisperDownloadClosed => {
					if let Some(model) = self.pending_download.take()
						&& !whisper_model_path(model.file_name).exists()
					{
						// download failed, set to selection to none
						par.general_config.whisper_model = "".into();
						par.config_change_kind.replace(ConfigChangeKind::Other);
						// reload the tab
						self.parent_tasks.push(ParentTask::SetTab(TabNameEnum::Features));
					}
				}
				Task::WhisperRemoveUnused => {
					let _ = whisper_delete_all_models().log_err("could not remove whisper models");
				}
				Task::WhisperDownload(model) => {
					self.pending_download = Some(model);
					self.show_whisper_download_dialog(model, par.executor.clone());
				}
				Task::WhisperDownloadDone => {
					if let Some(model) = self.pending_download.take() {
						par.general_config.whisper_model = model.file_name.into();
						par.config_change_kind.replace(ConfigChangeKind::Other);

						// reload tab so that the downloaded check marks get populated
						self.parent_tasks.push(ParentTask::SetTab(TabNameEnum::Features));
					}
				}
				Task::CloseDialog => {
					let close_dialog = self.popup_dialog.get_close_callback(par.layout);
					close_dialog();
				}
				Task::ReloadTab => {
					self.parent_tasks.push(ParentTask::SetTab(TabNameEnum::Features));
				}
			}
		}

		Ok(())
	}

	fn context_menu_custom(
		&mut self,
		action: Rc<str>,
		config: &mut GeneralConfig,
		change_kind: &mut Option<ConfigChangeKind>,
		layout: &mut wgui::layout::Layout,
		state: &mut wgui::parser::ParserState,
	) -> anyhow::Result<()> {
		let (Some(_), Some(_type), Some(id), Some(value)) = ({
			let mut s = action.splitn(4, ';');
			(s.next(), s.next(), s.next(), s.next())
		}) else {
			debug_assert!(false);
			return Ok(());
		};

		match _type {
			"whisper_model" => {
				let model_name = value;
				if !model_name.is_empty()
					&& let Some(model) = whisper_model_from_name(model_name)
				{
					let mut common = layout.common();
					let mut label = state.fetch_widget_as::<WidgetLabel>(common.state, &format!("{id}_value"))?;
					label.set_text(&mut common, Translation::from_raw_text(model.display_name));

					config.whisper_model = model.file_name.into();
					if !whisper_model_path(model.file_name).exists() {
						self.show_whisper_model_dialog_box_download(model)?;
					} else {
						change_kind.replace(ConfigChangeKind::Other);
					}
				} else {
					config.whisper_model = "".into();
					change_kind.replace(ConfigChangeKind::Other);
					// re-init the whole tab
					if whisper_any_models_downloaded().unwrap_or_default() {
						self.show_whisper_model_dialog_box_cleanup()?;
					} else {
						self.parent_tasks.push(ParentTask::SetTab(TabNameEnum::Features));
					}
				}
			}
			"whisper_gpu" => {
				let mut common = layout.common();
				let mut label = state.fetch_widget_as::<WidgetLabel>(common.state, &format!("{id}_value"))?;
				config.whisper_gpu = value.parse::<u32>().unwrap();
				label.set_text(
					&mut common,
					Translation::from_raw_text_string(config.whisper_gpu.to_string()),
				);
				change_kind.replace(ConfigChangeKind::Other);
			}
			_ => unreachable!(),
		};

		Ok(())
	}
}

impl State {
	pub fn mount(par: SettingsMountParams, vulkan_devices: Vec<String>) -> anyhow::Result<State> {
		let tasks = Tasks::<Task>::new();
		let popup_download = PopupHolder::<views::download_file::View>::default();
		let popup_dialog = PopupHolder::<views::dialog_box::View>::default();

		let c = options_category(
			par.mp,
			par.id_parent,
			"APP_SETTINGS.FEATURES",
			"@/dashboard/options.svg",
		)?;

		if par.feats.whisper {
			let c = options_category(par.mp, c, "APP_SETTINGS.SPEECH_TO_TEXT", "@/dashboard/stt.svg")?;
			whisper_models_dropdown(par.mp, c)?;
			whisper_gpu_select_dropdown(par.mp, c, vulkan_devices)?;
		}

		options_checkbox(par.mp, c, SettingType::NotificationsEnabled)?;
		options_checkbox(par.mp, c, SettingType::NotificationsSoundEnabled)?;
		options_checkbox(par.mp, c, SettingType::KeyboardSoundEnabled)?;
		if par.feats.xr_backend.is_open_vr() || par.feats.monado {
			// monado or openvr
			options_checkbox(par.mp, c, SettingType::BlockGameInput)?;
			options_checkbox(par.mp, c, SettingType::BlockGameInputIgnoreWatch)?;
		}
		if par.feats.monado {
			// monado-only
			options_checkbox(par.mp, c, SettingType::BlockPosesOnKbdInteraction)?;
		}

		options_range_f32(
			par.mp,
			c,
			SettingType::WatchViewAngleMin,
			SettingType::WatchViewAngleMax,
			0.1,
			1.0,
			0.1,
		)?;
		Ok(State {
			tasks,
			parent_tasks: par.mp.tasks.clone(),
			popup_download,
			popup_dialog,
			frontend_tasks: par.frontend_tasks.clone(),
			globals: par.mp.doc_params.globals.clone(),
			pending_download: None,
		})
	}

	fn show_whisper_download_dialog(&mut self, model: &WhisperModel, executor: AsyncExecutor) {
		views::download_file::mount_popup(
			self.popup_download.clone(),
			self.frontend_tasks.clone(),
			self.tasks.make_callback_box(Task::WhisperDownloadClosed),
			views::download_file::Params {
				globals: self.globals.clone(),
				executor,
				target_path: whisper_model_path(model.file_name),
				url: model.url.into(),
				on_downloaded: self.tasks.make_callback_box(Task::WhisperDownloadDone),
			},
		);
	}

	fn show_whisper_model_dialog_box_download(&mut self, model: &'static WhisperModel) -> anyhow::Result<()> {
		const ACTION_DOWNLOAD: &str = "download";
		const ACTION_CANCEL: &str = "cancel";

		self.pending_download = Some(model);

		let tasks = self.tasks.clone();
		views::dialog_box::mount_popup(
			self.popup_dialog.clone(),
			self.frontend_tasks.clone(),
			views::dialog_box::Params {
				globals: self.globals.clone(),
				message: Translation::from_translation_key("APP_SETTINGS.WHISPER.NEED_TO_DOWNLOAD_MODEL"),
				entries: vec![
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("APP_SETTINGS.CANCEL"),
						icon: "@/dashboard/close.svg",
						action: ACTION_CANCEL,
					},
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("DOWNLOAD"),
						icon: "@/dashboard/download.svg",
						action: ACTION_DOWNLOAD,
					},
				],
				on_action_click: Box::new(move |action| match action {
					ACTION_DOWNLOAD => {
						tasks.push(Task::WhisperDownload(model));
					}
					ACTION_CANCEL => {
						tasks.push(Task::CloseDialog);
						// treat as failed download
						tasks.push(Task::WhisperDownloadClosed);
					}
					_ => unreachable!(),
				}),
			},
		);

		Ok(())
	}

	fn show_whisper_model_dialog_box_cleanup(&mut self) -> anyhow::Result<()> {
		const ACTION_REMOVE: &str = "remove";
		const ACTION_CANCEL: &str = "cancel";

		let tasks = self.tasks.clone();

		views::dialog_box::mount_popup(
			self.popup_dialog.clone(),
			self.frontend_tasks.clone(),
			views::dialog_box::Params {
				globals: self.globals.clone(),
				message: Translation::from_translation_key("APP_SETTINGS.WHISPER.REMOVE_UNUSED_MODELS"),
				entries: vec![
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("APP_SETTINGS.CANCEL"),
						icon: "@/dashboard/close.svg",
						action: ACTION_CANCEL,
					},
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("REMOVE"),
						icon: "@/dashboard/trash.svg",
						action: ACTION_REMOVE,
					},
				],
				on_action_click: Box::new(move |action| match action {
					ACTION_REMOVE => {
						tasks.push(Task::WhisperRemoveUnused);
						tasks.push(Task::CloseDialog);
						tasks.push(Task::ReloadTab);
					}
					ACTION_CANCEL => {
						tasks.push(Task::CloseDialog);
						tasks.push(Task::ReloadTab);
					}
					_ => unreachable!(),
				}),
			},
		);

		Ok(())
	}
}

fn whisper_gpu_select_dropdown(
	mp: &mut MacroParams,
	parent: WidgetID,
	vulkan_devices: Vec<String>,
) -> anyhow::Result<()> {
	let id = mp.idx.to_string();
	mp.idx += 1;

	let parent = horiz_cell(mp.layout, parent)?;

	let mut params = TemplateParams::new();
	params.insert("translation", "APP_SETTINGS.WHISPER_GPU_SELECT");

	let current_text = Translation::from_raw_text_string(mp.config.whisper_gpu.to_string());

	let cells = vulkan_devices
		.iter()
		.enumerate()
		.filter(|(_, name)| !name.contains("llvmpipe"))
		.map(|(idx, name)| {
			context_menu::Cell::Button(context_menu::Button {
				action_name: Some(format!(";whisper_gpu;{};{}", id, idx).into()),
				title: Translation::from_raw_text_string(name.to_string()),
				..Default::default()
			})
		})
		.collect();

	let tasks = mp.tasks.clone();
	wgui_simple::mount_dropdown(wgui_simple::MountDropdownParams {
		layout: mp.layout,
		parser_state: mp.parser_state,
		doc_params: mp.doc_params,
		parent,
		id,
		params,
		current_text,
		cells,
		on_open: Box::new(move |pos, cells| tasks.push(ParentTask::OpenContextMenu(pos, cells))),
	})
}

fn whisper_models_dropdown(mp: &mut MacroParams, parent: WidgetID) -> anyhow::Result<()> {
	let id = mp.idx.to_string();
	mp.idx += 1;

	let parent = horiz_cell(mp.layout, parent)?;

	let mut downloaded_models = vec![];
	for (idx, m) in WHISPER_MODELS.iter().enumerate() {
		if whisper_model_path(m.file_name).exists() {
			downloaded_models.push(idx);
		}
	}

	let current_file = mp.config.whisper_model.as_ref();
	let current_translation = whisper_model_from_name(current_file)
		.map_or(Translation::from_translation_key("APP_SETTINGS.OPTION.NONE"), |x| {
			Translation::from_raw_text(x.display_name)
		});

	let mut params = TemplateParams::new();
	params.insert("translation", "APP_SETTINGS.WHISPER_MODEL");
	params.insert("tooltip", "APP_SETTINGS.WHISPER_MODEL_HELP");

	let mut cells = WHISPER_MODELS
		.iter()
		.enumerate()
		.map(|(idx, item)| {
			if downloaded_models.contains(&idx) {
				context_menu::Cell::Button(context_menu::Button {
					action_name: Some(format!(";whisper_model;{id};{}", item.file_name).into()),
					title: Translation::from_raw_text(item.display_name),
					sprite_src_builtin: Some("@/dashboard/check.svg".into()),
					..Default::default()
				})
			} else {
				context_menu::Cell::Button(context_menu::Button {
					action_name: Some(format!(";whisper_model;{id};{}", item.file_name).into()),
					title: Translation::from_raw_text(item.display_name),
					..Default::default()
				})
			}
		})
		.collect::<Vec<_>>();

	cells.insert(
		0,
		context_menu::Cell::Button(context_menu::Button {
			action_name: Some(format!(";whisper_model;{id};").into()),
			title: Translation::from_translation_key("APP_SETTINGS.OPTION.NONE"),
			..Default::default()
		}),
	);

	let tasks = mp.tasks.clone();
	wgui_simple::mount_dropdown(wgui_simple::MountDropdownParams {
		layout: mp.layout,
		parser_state: mp.parser_state,
		doc_params: mp.doc_params,
		parent,
		id,
		params,
		current_text: current_translation,
		cells,
		on_open: Box::new(move |pos, cells| tasks.push(ParentTask::OpenContextMenu(pos, cells))),
	})
}
