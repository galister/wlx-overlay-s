use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use chrono::Timelike;
use glam::Vec2;
use wgui::{
	components::button::ComponentButton,
	event::{CallbackDataCommon, EventAlterables, EventListenerCollection},
	globals::WguiGlobals,
	i18n::Translation,
	layout::{LayoutParams, RcLayout, WidgetID},
	parser::{ParseDocumentParams, ParserState},
	widget::label::WidgetLabel,
};

use crate::tab::{
	Tab, TabParams, TabType, apps::TabApps, games::TabGames, home::TabHome, monado::TabMonado, processes::TabProcesses,
	settings::TabSettings,
};

mod assets;
mod tab;
mod various;

pub struct Frontend {
	pub layout: RcLayout,
	globals: WguiGlobals,

	#[allow(dead_code)]
	state: ParserState,

	current_tab: Option<Box<dyn Tab>>,

	tasks: VecDeque<FrontendTask>,

	ticks: u32,

	label_time_id: WidgetID,
}

pub type RcFrontend = Rc<RefCell<Frontend>>;

pub enum FrontendTask {
	SetTab(TabType),
}

pub struct FrontendParams<'a> {
	pub listeners: &'a mut EventListenerCollection<(), ()>,
}

impl Frontend {
	pub fn new(params: FrontendParams) -> anyhow::Result<(RcFrontend, RcLayout)> {
		let globals = WguiGlobals::new(Box::new(assets::Asset {}), wgui::globals::Defaults::default())?;

		let (layout, state) = wgui::parser::new_layout_from_assets(
			params.listeners,
			&ParseDocumentParams {
				globals: globals.clone(),
				path: "gui/dashboard.xml",
				extra: Default::default(),
			},
			&LayoutParams { resize_to_parent: true },
		)?;

		let rc_layout = layout.as_rc();

		let mut tasks = VecDeque::<FrontendTask>::new();
		tasks.push_back(FrontendTask::SetTab(TabType::Home));

		let label_time_id = state.get_widget_id("label_time")?;

		let res = Rc::new(RefCell::new(Self {
			layout: rc_layout.clone(),
			state,
			current_tab: None,
			globals,
			tasks,
			ticks: 0,
			label_time_id,
		}));

		Frontend::register_widgets(&res)?;

		Ok((res, rc_layout))
	}

	pub fn update(
		&mut self,
		rc_this: &RcFrontend,
		listeners: &mut EventListenerCollection<(), ()>,
		width: f32,
		height: f32,
		timestep_alpha: f32,
	) -> anyhow::Result<()> {
		while let Some(task) = self.tasks.pop_front() {
			self.process_task(rc_this, task, listeners)?;
		}

		self.tick(width, height, timestep_alpha)?;
		self.ticks += 1;

		Ok(())
	}

	fn tick(&mut self, width: f32, height: f32, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut layout = self.layout.borrow_mut();

		let mut alterables = EventAlterables::default();
		let mut common = CallbackDataCommon {
			alterables: &mut alterables,
			state: &layout.state,
		};

		// fixme: timer events instead of this thing
		if self.ticks.is_multiple_of(1000) {
			self.update_time(&mut common);
		}

		layout.update(Vec2::new(width, height), timestep_alpha)?;
		layout.process_alterables(alterables)?;

		Ok(())
	}

	fn update_time(&self, common: &mut CallbackDataCommon) {
		let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(self.label_time_id) else {
			return;
		};

		let now = chrono::Local::now();
		let hours = now.hour();
		let minutes = now.minute();

		label.set_text(common, Translation::from_raw_text(&format!("{hours:02}:{minutes:02}")));
	}

	pub fn get_layout(&self) -> &RcLayout {
		&self.layout
	}

	pub fn push_task(&mut self, task: FrontendTask) {
		self.tasks.push_back(task);
	}

	fn process_task(
		&mut self,
		rc_this: &RcFrontend,
		task: FrontendTask,
		listeners: &mut EventListenerCollection<(), ()>,
	) -> anyhow::Result<()> {
		match task {
			FrontendTask::SetTab(tab_type) => self.set_tab(tab_type, rc_this, listeners)?,
		}
		Ok(())
	}

	fn set_tab(
		&mut self,
		tab_type: TabType,
		rc_this: &RcFrontend,
		listeners: &mut EventListenerCollection<(), ()>,
	) -> anyhow::Result<()> {
		log::info!("Setting tab to {tab_type:?}");
		let mut layout = self.layout.borrow_mut();
		let widget_content = self.state.fetch_widget(&layout.state, "content")?;
		layout.remove_children(widget_content.id);

		let tab_params = TabParams {
			globals: &self.globals,
			layout: &mut layout,
			parent_id: widget_content.id,
			listeners,
			frontend: rc_this,
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
