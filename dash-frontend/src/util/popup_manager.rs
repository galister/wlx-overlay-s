use std::{
	cell::RefCell,
	rc::{Rc, Weak},
};

use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	event::{EventAlterables, StyleSetRequest},
	globals::WguiGlobals,
	layout::{Layout, LayoutTask, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState},
	taffy::Display,
};

pub struct PopupManagerParams<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
}

struct MountedPopup {
	#[allow(dead_code)]
	state: ParserState,
	id_root: WidgetID,
}

pub struct State {
	popup_stack: Vec<MountedPopup>,
}

pub struct PopupManager {
	pub state: Rc<RefCell<State>>,
	globals: WguiGlobals,
	parent_id: WidgetID,
}

pub struct PushPopupResult {
	pub id_content: WidgetID,
}

impl State {
	fn refresh_stack(&mut self, alterables: &mut EventAlterables) {
		// show only the topmost popup
		for (idx, popup) in self.popup_stack.iter().enumerate() {
			alterables.set_style(
				popup.id_root,
				StyleSetRequest::Display(if idx == self.popup_stack.len() - 1 {
					Display::Flex
				} else {
					Display::None
				}),
			);
		}
	}

	fn pop_popup(&mut self, alterables: &mut EventAlterables) {
		let Some(popup) = self.popup_stack.pop() else {
			return;
		};

		alterables.tasks.push(LayoutTask::RemoveWidget(popup.id_root));
		self.refresh_stack(alterables);
	}
}

impl PopupManager {
	pub fn new(params: PopupManagerParams) -> anyhow::Result<Self> {
		Ok(Self {
			globals: params.globals,
			parent_id: params.parent_id,
			state: Rc::new(RefCell::new(State {
				popup_stack: Vec::new(),
			})),
		})
	}

	pub fn push_popup(&mut self, globals: WguiGlobals, layout: &mut Layout) -> anyhow::Result<PushPopupResult> {
		let doc_params = &ParseDocumentParams {
			globals,
			path: AssetPath::BuiltIn("gui/view/popup_window.xml"),
			extra: Default::default(),
		};
		let state = wgui::parser::parse_from_assets(doc_params, layout, self.parent_id)?;

		let id_root = state.get_widget_id("root")?;
		let id_content = state.get_widget_id("content")?;

		let but_back = state.fetch_component_as::<ComponentButton>("but_back")?;

		but_back.on_click({
			let state = self.state.clone();
			Box::new(move |common, _evt| {
				state.borrow_mut().pop_popup(common.alterables);
				Ok(())
			})
		});

		let mounted_popup = MountedPopup { state, id_root };

		let mut state = self.state.borrow_mut();
		state.popup_stack.push(mounted_popup);

		let mut c = layout.start_common();
		state.refresh_stack(c.common().alterables);
		c.finish()?;

		Ok(PushPopupResult { id_content })
	}
}
