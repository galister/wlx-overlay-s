use std::{collections::HashMap, marker::PhantomData, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::checkbox::ComponentCheckbox,
	globals::WguiGlobals,
	layout::WidgetID,
	parser::{self, Fetchable, ParseDocumentParams, ParserState},
	task::Tasks,
};
use wlx_common::dash_interface;

use crate::{
	frontend::Frontend,
	tab::{Tab, TabType},
};

#[derive(Debug)]
enum Task {
	Refresh,
	FocusClient(String),
}

pub struct TabMonado<T> {
	#[allow(dead_code)]
	state: ParserState,
	tasks: Tasks<Task>,

	marker: PhantomData<T>,

	globals: WguiGlobals,
	id_list_parent: WidgetID,

	cells: Vec<parser::ParserData>,

	ticks: u32,
}

impl<T> Tab<T> for TabMonado<T> {
	fn get_type(&self) -> TabType {
		TabType::Games
	}

	fn update(&mut self, frontend: &mut Frontend<T>, data: &mut T) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::Refresh => self.refresh(frontend, data)?,
				Task::FocusClient(name) => self.focus_client(frontend, data, name)?,
			}
		}

		// every few seconds
		if self.ticks.is_multiple_of(500) {
			self.tasks.push(Task::Refresh);
		}

		self.ticks += 1;

		Ok(())
	}
}

fn doc_params(globals: &'_ WguiGlobals) -> ParseDocumentParams<'_> {
	ParseDocumentParams {
		globals: globals.clone(),
		path: AssetPath::BuiltIn("gui/tab/monado.xml"),
		extra: Default::default(),
	}
}

fn yesno(n: bool) -> &'static str {
	match n {
		true => "yes",
		false => "no",
	}
}

impl<T> TabMonado<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID) -> anyhow::Result<Self> {
		let globals = frontend.layout.state.globals.clone();
		let state = wgui::parser::parse_from_assets(&doc_params(&globals), &mut frontend.layout, parent_id)?;

		let id_list_parent = state.get_widget_id("list_parent")?;

		let tasks = Tasks::<Task>::new();

		tasks.push(Task::Refresh);

		Ok(Self {
			state,
			marker: PhantomData,
			tasks,
			globals,
			id_list_parent,
			ticks: 0,
			cells: Vec::new(),
		})
	}

	fn mount_client(&mut self, frontend: &mut Frontend<T>, client: &dash_interface::MonadoClient) -> anyhow::Result<()> {
		let mut par = HashMap::<Rc<str>, Rc<str>>::new();
		par.insert(
			"checked".into(),
			if client.is_primary {
				Rc::from("1")
			} else {
				Rc::from("0")
			},
		);
		par.insert("name".into(), client.name.clone().into());
		par.insert("flag_active".into(), yesno(client.is_active).into());
		par.insert("flag_focused".into(), yesno(client.is_focused).into());
		par.insert("flag_io_active".into(), yesno(client.is_io_active).into());
		par.insert("flag_overlay".into(), yesno(client.is_overlay).into());
		par.insert("flag_primary".into(), yesno(client.is_primary).into());
		par.insert("flag_visible".into(), yesno(client.is_visible).into());

		let state_cell = self.state.parse_template(
			&doc_params(&self.globals),
			"Cell",
			&mut frontend.layout,
			self.id_list_parent,
			par,
		)?;

		let checkbox = state_cell.fetch_component_as::<ComponentCheckbox>("checkbox")?;
		checkbox.on_toggle({
			let tasks = self.tasks.clone();
			let client_name = client.name.clone();
			Box::new(move |_common, e| {
				if e.checked {
					tasks.push(Task::FocusClient(client_name.clone()));
				}
				Ok(())
			})
		});

		self.cells.push(state_cell);

		Ok(())
	}

	fn refresh(&mut self, frontend: &mut Frontend<T>, data: &mut T) -> anyhow::Result<()> {
		log::debug!("refreshing monado client list");

		let clients = frontend.interface.monado_client_list(data)?;

		frontend.layout.remove_children(self.id_list_parent);
		self.cells.clear();

		for client in clients {
			self.mount_client(frontend, &client)?;
		}

		Ok(())
	}

	fn focus_client(&mut self, frontend: &mut Frontend<T>, data: &mut T, name: String) -> anyhow::Result<()> {
		frontend.interface.monado_client_focus(data, &name)?;
		self.tasks.push(Task::Refresh);
		Ok(())
	}
}
