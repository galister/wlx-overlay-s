use std::{path::PathBuf, rc::Rc};

use chrono::Timelike;
use glam::Vec2;
use wgui::{
	assets::{AssetPath, AssetProvider},
	components::button::ComponentButton,
	font_config::WguiFontConfig,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, LayoutParams, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
	windowing::{WguiWindow, WguiWindowParams, WguiWindowParamsExtra, WguiWindowPlacement},
};
use wlx_common::{dash_interface::BoxDashInterface, timestep::Timestep};

use crate::{
	assets, settings,
	tab::{
		apps::TabApps, games::TabGames, home::TabHome, monado::TabMonado, processes::TabProcesses, settings::TabSettings,
		Tab, TabType,
	},
	util::{
		desktop_finder::DesktopFinder,
		popup_manager::{MountPopupParams, PopupManager, PopupManagerParams},
		toast_manager::ToastManager,
		various::AsyncExecutor,
	},
	views,
};

pub struct FrontendWidgets {
	pub id_label_time: WidgetID,
	pub id_rect_content: WidgetID,
}

pub type FrontendTasks = Tasks<FrontendTask>;

pub struct Frontend {
	pub layout: Layout,
	globals: WguiGlobals,

	pub settings: Box<dyn settings::SettingsIO>,
	pub interface: BoxDashInterface,

	// async runtime executor
	pub executor: AsyncExecutor,

	#[allow(dead_code)]
	state: ParserState,

	current_tab: Option<Box<dyn Tab>>,

	pub tasks: FrontendTasks,

	ticks: u32,

	widgets: FrontendWidgets,
	popup_manager: PopupManager,
	toast_manager: ToastManager,
	timestep: Timestep,

	window_audio_settings: WguiWindow,
	view_audio_settings: Option<views::audio_settings::View>,

	pub(crate) desktop_finder: DesktopFinder,
}

pub struct InitParams {
	pub settings: Box<dyn settings::SettingsIO>,
	pub interface: BoxDashInterface,
}

#[derive(Clone)]
pub enum FrontendTask {
	SetTab(TabType),
	RefreshClock,
	RefreshBackground,
	MountPopup(MountPopupParams),
	RefreshPopupManager,
	ShowAudioSettings,
	UpdateAudioSettingsView,
	RecenterPlayspace,
	PushToast(Translation),
}

impl Frontend {
	pub fn new(params: InitParams) -> anyhow::Result<Frontend> {
		let mut assets = Box::new(assets::Asset {});

		let font_binary_bold = assets.load_from_path_gzip("Quicksand-Bold.ttf.gz")?;
		let font_binary_regular = assets.load_from_path_gzip("Quicksand-Regular.ttf.gz")?;
		let font_binary_light = assets.load_from_path_gzip("Quicksand-Light.ttf.gz")?;

		let globals = WguiGlobals::new(
			assets,
			wgui::globals::Defaults::default(),
			&WguiFontConfig {
				binaries: vec![&font_binary_regular, &font_binary_bold, &font_binary_light],
				family_name_sans_serif: "Quicksand",
				family_name_serif: "Quicksand",
				family_name_monospace: "",
			},
			PathBuf::new(), //FIXME: pass from somewhere else
		)?;

		let (layout, state) = wgui::parser::new_layout_from_assets(
			&ParseDocumentParams {
				globals: globals.clone(),
				path: AssetPath::BuiltIn("gui/dashboard.xml"),
				extra: Default::default(),
			},
			&LayoutParams { resize_to_parent: true },
		)?;

		let id_popup_manager = state.get_widget_id("popup_manager")?;
		let popup_manager = PopupManager::new(PopupManagerParams {
			parent_id: id_popup_manager,
		});

		let toast_manager = ToastManager::new();

		let tasks = FrontendTasks::new();
		tasks.push(FrontendTask::SetTab(TabType::Home));

		let id_label_time = state.get_widget_id("label_time")?;
		let id_rect_content = state.get_widget_id("rect_content")?;

		let timestep = Timestep::new(60.0);

		let mut desktop_finder = DesktopFinder::new();
		desktop_finder.refresh();

		let mut frontend = Self {
			layout,
			state,
			current_tab: None,
			globals,
			tasks,
			ticks: 0,
			widgets: FrontendWidgets {
				id_label_time,
				id_rect_content,
			},
			timestep,
			settings: params.settings,
			interface: params.interface,
			popup_manager,
			toast_manager,
			window_audio_settings: WguiWindow::default(),
			view_audio_settings: None,
			executor: Rc::new(smol::LocalExecutor::new()),
			desktop_finder,
		};

		// init some things first
		frontend.update_background()?;
		frontend.update_time()?;

		Frontend::register_widgets(&mut frontend)?;

		Ok(frontend)
	}

	pub fn update(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut tasks = self.tasks.drain();

		while let Some(task) = tasks.pop_front() {
			self.process_task(task)?;
		}

		if let Some(mut tab) = self.current_tab.take() {
			tab.update(self)?;

			self.current_tab = Some(tab);
		}

		// process async runtime tasks
		while self.executor.try_tick() {}

		self.tick(width, height, timestep_alpha)?;
		self.ticks += 1;

		Ok(())
	}

	fn tick(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		// fixme: timer events instead of this thing
		if self.ticks.is_multiple_of(1000) {
			self.update_time()?;
		}

		{
			// always 30 times per second
			while self.timestep.on_tick() {
				self.toast_manager.tick(&self.globals, &mut self.layout)?;
			}

			self.layout.update(Vec2::new(width, height), timestep_alpha)?;
		}

		Ok(())
	}

	fn update_time(&mut self) -> anyhow::Result<()> {
		let mut c = self.layout.start_common();
		let mut common = c.common();

		{
			let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(self.widgets.id_label_time) else {
				anyhow::bail!("");
			};

			let now = chrono::Local::now();
			let hours = now.hour();
			let minutes = now.minute();

			let text: String = if !self.settings.get().general.am_pm_clock {
				format!("{hours:02}:{minutes:02}")
			} else {
				let hours_ampm = (hours + 11) % 12 + 1;
				let suffix = if hours >= 12 { "PM" } else { "AM" };
				format!("{hours_ampm:02}:{minutes:02} {suffix}")
			};

			label.set_text(&mut common, Translation::from_raw_text(&text));
		}

		c.finish()?;
		Ok(())
	}

	fn mount_popup(&mut self, params: MountPopupParams) -> anyhow::Result<()> {
		self.popup_manager.mount_popup(
			self.globals.clone(),
			self.settings.as_ref(),
			&mut self.layout,
			&mut self.interface,
			self.tasks.clone(),
			params,
		)?;
		Ok(())
	}

	fn refresh_popup_manager(&mut self) -> anyhow::Result<()> {
		let mut c = self.layout.start_common();
		self.popup_manager.refresh(c.common().alterables);
		c.finish()?;
		Ok(())
	}

	fn update_background(&self) -> anyhow::Result<()> {
		let Some(mut rect) = self
			.layout
			.state
			.widgets
			.get_as::<WidgetRectangle>(self.widgets.id_rect_content)
		else {
			anyhow::bail!("");
		};

		let (alpha1, alpha2) = if !self.settings.get().general.opaque_background {
			(0.8666, 0.9333)
		} else {
			(1.0, 1.0)
		};

		rect.params.color.a = alpha1;
		rect.params.color2.a = alpha2;

		Ok(())
	}

	fn process_task(&mut self, task: FrontendTask) -> anyhow::Result<()> {
		match task {
			FrontendTask::SetTab(tab_type) => self.set_tab(tab_type)?,
			FrontendTask::RefreshClock => self.update_time()?,
			FrontendTask::RefreshBackground => self.update_background()?,
			FrontendTask::MountPopup(params) => self.mount_popup(params)?,
			FrontendTask::RefreshPopupManager => self.refresh_popup_manager()?,
			FrontendTask::ShowAudioSettings => self.action_show_audio_settings()?,
			FrontendTask::UpdateAudioSettingsView => self.action_update_audio_settings()?,
			FrontendTask::RecenterPlayspace => self.action_recenter_playspace()?,
			FrontendTask::PushToast(content) => self.toast_manager.push(content),
		};
		Ok(())
	}

	fn set_tab(&mut self, tab_type: TabType) -> anyhow::Result<()> {
		log::info!("Setting tab to {tab_type:?}");
		let widget_content = self.state.fetch_widget(&self.layout.state, "content")?;
		self.layout.remove_children(widget_content.id);

		let tab: Box<dyn Tab> = match tab_type {
			TabType::Home => Box::new(TabHome::new(self, widget_content.id)?),
			TabType::Apps => Box::new(TabApps::new(self, widget_content.id)?),
			TabType::Games => Box::new(TabGames::new(self, widget_content.id)?),
			TabType::Monado => Box::new(TabMonado::new(self, widget_content.id)?),
			TabType::Processes => Box::new(TabProcesses::new(self, widget_content.id)?),
			TabType::Settings => Box::new(TabSettings::new(self, widget_content.id)?),
		};

		self.current_tab = Some(tab);

		Ok(())
	}

	fn register_widgets(&mut self) -> anyhow::Result<()> {
		// ################################
		// SIDE BUTTONS
		// ################################

		// "Home" side button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_home")?,
			FrontendTask::SetTab(TabType::Home),
		);

		// "Apps" side button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_apps")?,
			FrontendTask::SetTab(TabType::Apps),
		);

		// "Games" side button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_games")?,
			FrontendTask::SetTab(TabType::Games),
		);

		// "Monado side button"
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_monado")?,
			FrontendTask::SetTab(TabType::Monado),
		);

		// "Processes" side button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_processes")?,
			FrontendTask::SetTab(TabType::Processes),
		);

		// "Settings" side button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_side_settings")?,
			FrontendTask::SetTab(TabType::Settings),
		);

		// ################################
		// BOTTOM BAR BUTTONS
		// ################################

		// "Audio" bottom bar button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_audio")?,
			FrontendTask::ShowAudioSettings,
		);

		// "Recenter playspace" bottom bar button
		self.tasks.handle_button(
			&self.state.fetch_component_as::<ComponentButton>("btn_recenter")?,
			FrontendTask::RecenterPlayspace,
		);

		Ok(())
	}

	fn action_show_audio_settings(&mut self) -> anyhow::Result<()> {
		self.window_audio_settings.open(&mut WguiWindowParams {
			globals: self.globals.clone(),
			position: Vec2::new(64.0, 64.0),
			layout: &mut self.layout,
			title: Translation::from_translation_key("AUDIO.SETTINGS"),
			extra: WguiWindowParamsExtra {
				fixed_width: Some(400.0),
				placement: WguiWindowPlacement::BottomLeft,
				..Default::default()
			},
		})?;

		let content = self.window_audio_settings.get_content();

		self.view_audio_settings = Some(views::audio_settings::View::new(views::audio_settings::Params {
			globals: self.globals.clone(),
			frontend_tasks: self.tasks.clone(),
			layout: &mut self.layout,
			parent_id: content.id,
			on_update: {
				let tasks = self.tasks.clone();
				Rc::new(move || {
					tasks.push(FrontendTask::UpdateAudioSettingsView);
				})
			},
		})?);
		Ok(())
	}

	fn action_update_audio_settings(&mut self) -> anyhow::Result<()> {
		let Some(view) = &mut self.view_audio_settings else {
			return Ok(());
		};

		view.update(&mut self.layout)?;

		Ok(())
	}

	fn action_recenter_playspace(&mut self) -> anyhow::Result<()> {
		self.interface.recenter_playspace()?;
		Ok(())
	}
}
