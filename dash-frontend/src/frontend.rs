use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use chrono::Timelike;
use glam::Vec2;
use wgui::{
	assets::{AssetPath, AssetProvider},
	components::button::ComponentButton,
	font_config::WguiFontConfig,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{LayoutParams, RcLayout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	widget::{label::WidgetLabel, rectangle::WidgetRectangle},
};

use crate::{
	assets, settings,
	tab::{
		Tab, TabParams, TabType, apps::TabApps, games::TabGames, home::TabHome, monado::TabMonado, processes::TabProcesses,
		settings::TabSettings,
	},
	util::popup_manager::{MountPopupParams, PopupManager, PopupManagerParams},
};

pub struct FrontendWidgets {
	pub id_label_time: WidgetID,
	pub id_rect_content: WidgetID,
}

#[derive(Clone)]
pub struct FrontendTasks(pub Rc<RefCell<VecDeque<FrontendTask>>>);

impl FrontendTasks {
	fn new() -> Self {
		Self(Rc::new(RefCell::new(VecDeque::new())))
	}

	pub fn push(&self, task: FrontendTask) {
		self.0.borrow_mut().push_back(task);
	}
}

pub struct Frontend {
	pub layout: RcLayout,
	globals: WguiGlobals,

	pub settings: Box<dyn settings::SettingsIO>,

	#[allow(dead_code)]
	state: ParserState,

	current_tab: Option<Box<dyn Tab>>,

	pub tasks: FrontendTasks,

	ticks: u32,

	widgets: FrontendWidgets,
	popup_manager: PopupManager,
}

pub struct InitParams {
	pub settings: Box<dyn settings::SettingsIO>,
}

pub type RcFrontend = Rc<RefCell<Frontend>>;

pub enum FrontendTask {
	SetTab(TabType),
	RefreshClock,
	RefreshBackground,
	MountPopup(MountPopupParams),
	RefreshPopupManager,
}

impl Frontend {
	pub fn new(params: InitParams) -> anyhow::Result<(RcFrontend, RcLayout)> {
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
		)?;

		let (mut layout, state) = wgui::parser::new_layout_from_assets(
			&ParseDocumentParams {
				globals: globals.clone(),
				path: AssetPath::BuiltIn("gui/dashboard.xml"),
				extra: Default::default(),
			},
			&LayoutParams { resize_to_parent: true },
		)?;

		let id_popup_manager = state.get_widget_id("popup_manager")?;
		let popup_manager = PopupManager::new(PopupManagerParams {
			globals: globals.clone(),
			layout: &mut layout,
			parent_id: id_popup_manager,
		})?;

		let rc_layout = layout.as_rc();

		let tasks = FrontendTasks::new();
		tasks.push(FrontendTask::SetTab(TabType::Home));

		let id_label_time = state.get_widget_id("label_time")?;
		let id_rect_content = state.get_widget_id("rect_content")?;

		let frontend = Self {
			layout: rc_layout.clone(),
			state,
			current_tab: None,
			globals,
			tasks,
			ticks: 0,
			widgets: FrontendWidgets {
				id_label_time,
				id_rect_content,
			},
			settings: params.settings,
			popup_manager,
		};

		// init some things first
		frontend.update_background()?;
		frontend.update_time()?;

		let res = Rc::new(RefCell::new(frontend));

		Frontend::register_widgets(&res)?;

		Ok((res, rc_layout))
	}

	pub fn update(&mut self, rc_this: &RcFrontend, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut tasks = {
			let mut tasks = self.tasks.0.borrow_mut();
			std::mem::take(&mut *tasks)
		};

		while let Some(task) = tasks.pop_front() {
			self.process_task(rc_this, task)?;
		}

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
			let mut layout = self.layout.borrow_mut();
			layout.update(Vec2::new(width, height), timestep_alpha)?;
		}

		Ok(())
	}

	fn update_time(&self) -> anyhow::Result<()> {
		let mut layout = self.layout.borrow_mut();
		let mut c = layout.start_common();
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
		let mut layout = self.layout.borrow_mut();
		self
			.popup_manager
			.mount_popup(self.globals.clone(), &mut layout, self.tasks.clone(), params)?;
		Ok(())
	}

	fn refresh_popup_manager(&mut self) -> anyhow::Result<()> {
		let mut layout = self.layout.borrow_mut();
		let mut c = layout.start_common();
		self.popup_manager.refresh(c.common().alterables);
		c.finish()?;
		Ok(())
	}

	fn update_background(&self) -> anyhow::Result<()> {
		let layout = self.layout.borrow_mut();

		let Some(mut rect) = layout
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

	pub fn get_layout(&self) -> &RcLayout {
		&self.layout
	}

	fn process_task(&mut self, rc_this: &RcFrontend, task: FrontendTask) -> anyhow::Result<()> {
		match task {
			FrontendTask::SetTab(tab_type) => self.set_tab(tab_type, rc_this)?,
			FrontendTask::RefreshClock => self.update_time()?,
			FrontendTask::RefreshBackground => self.update_background()?,
			FrontendTask::MountPopup(params) => self.mount_popup(params)?,
			FrontendTask::RefreshPopupManager => self.refresh_popup_manager()?,
		}
		Ok(())
	}

	fn set_tab(&mut self, tab_type: TabType, rc_this: &RcFrontend) -> anyhow::Result<()> {
		log::info!("Setting tab to {tab_type:?}");
		let mut layout = self.layout.borrow_mut();
		let widget_content = self.state.fetch_widget(&layout.state, "content")?;
		layout.remove_children(widget_content.id);

		let tab_params = TabParams {
			globals: &self.globals,
			layout: &mut layout,
			parent_id: widget_content.id,
			frontend: rc_this,
			frontend_widgets: &self.widgets,
			settings: self.settings.get_mut(),
		};

		let tab: Box<dyn Tab> = match tab_type {
			TabType::Home => Box::new(TabHome::new(tab_params)?),
			TabType::Apps => Box::new(TabApps::new(tab_params)?),
			TabType::Games => Box::new(TabGames::new(tab_params)?),
			TabType::Monado => Box::new(TabMonado::new(tab_params)?),
			TabType::Processes => Box::new(TabProcesses::new(tab_params)?),
			TabType::Settings => Box::new(TabSettings::new(tab_params)?),
		};

		self.current_tab = Some(tab);

		Ok(())
	}

	fn register_widgets(rc_this: &RcFrontend) -> anyhow::Result<()> {
		let this = rc_this.borrow_mut();
		let btn_home = this.state.fetch_component_as::<ComponentButton>("btn_side_home")?;
		let btn_apps = this.state.fetch_component_as::<ComponentButton>("btn_side_apps")?;
		let btn_games = this.state.fetch_component_as::<ComponentButton>("btn_side_games")?;
		let btn_monado = this.state.fetch_component_as::<ComponentButton>("btn_side_monado")?;
		let btn_processes = this.state.fetch_component_as::<ComponentButton>("btn_side_processes")?;
		let btn_settings = this.state.fetch_component_as::<ComponentButton>("btn_side_settings")?;

		TabType::register_button(rc_this.clone(), &btn_home, TabType::Home);
		TabType::register_button(rc_this.clone(), &btn_apps, TabType::Apps);
		TabType::register_button(rc_this.clone(), &btn_games, TabType::Games);
		TabType::register_button(rc_this.clone(), &btn_monado, TabType::Monado);
		TabType::register_button(rc_this.clone(), &btn_processes, TabType::Processes);
		TabType::register_button(rc_this.clone(), &btn_settings, TabType::Settings);
		Ok(())
	}
}
