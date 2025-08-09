use std::{collections::VecDeque, rc::Rc, sync::Arc};

use crate::{
	animation::Animations,
	components::{Component, InitData},
	event::{self, EventAlterables, EventListenerCollection},
	globals::WguiGlobals,
	transform_stack::Transform,
	widget::{self, EventParams, WidgetObj, WidgetState, div::Div},
};

use glam::{Vec2, vec2};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use slotmap::{HopSlotMap, SecondaryMap, new_key_type};
use taffy::{TaffyTree, TraversePartialTree};

new_key_type! {
	pub struct WidgetID;
}

pub type BoxWidget = Arc<Mutex<WidgetState>>;

pub struct WidgetMap(HopSlotMap<WidgetID, BoxWidget>);
pub type WidgetNodeMap = SecondaryMap<WidgetID, taffy::NodeId>;

impl WidgetMap {
	fn new() -> Self {
		Self(HopSlotMap::with_key())
	}

	pub fn get_as<T: 'static>(&self, handle: WidgetID) -> Option<MappedMutexGuard<T>> {
		let widget = self.0.get(handle)?;
		Some(MutexGuard::map(widget.lock(), |w| w.obj.get_as_mut::<T>()))
	}

	pub fn get(&self, handle: WidgetID) -> Option<&BoxWidget> {
		self.0.get(handle)
	}

	pub fn insert(&mut self, obj: BoxWidget) -> WidgetID {
		self.0.insert(obj)
	}

	// cast to specific widget type, does nothing if widget ID is expired
	// panics in case if the widget type is wrong
	pub fn call<WIDGET, FUNC>(&self, widget_id: WidgetID, func: FUNC)
	where
		WIDGET: WidgetObj,
		FUNC: FnOnce(&mut WIDGET),
	{
		let Some(widget) = self.get(widget_id) else {
			debug_assert!(false);
			return;
		};

		let mut lock = widget.lock();
		let m = lock.obj.get_as_mut::<WIDGET>();

		func(m);
	}
}

pub struct LayoutState {
	pub globals: WguiGlobals,
	pub widgets: WidgetMap,
	pub nodes: WidgetNodeMap,
	pub tree: taffy::tree::TaffyTree<WidgetID>,
	pub alterables: EventAlterables,
}

pub struct Layout {
	pub state: LayoutState,

	pub components_to_init: VecDeque<Rc<dyn Component>>,

	pub root_widget: WidgetID,
	pub root_node: taffy::NodeId,

	pub prev_size: Vec2,
	pub content_size: Vec2,

	pub needs_redraw: bool,
	pub haptics_triggered: bool,

	pub animations: Animations,
}

fn add_child_internal(
	tree: &mut taffy::TaffyTree<WidgetID>,
	widgets: &mut WidgetMap,
	nodes: &mut WidgetNodeMap,
	parent_node: Option<taffy::NodeId>,
	widget: WidgetState,
	style: taffy::Style,
) -> anyhow::Result<(WidgetID, taffy::NodeId)> {
	#[allow(clippy::arc_with_non_send_sync)]
	let child_id = widgets.insert(Arc::new(Mutex::new(widget)));
	let child_node = tree.new_leaf_with_context(style, child_id)?;

	if let Some(parent_node) = parent_node {
		tree.add_child(parent_node, child_node)?;
	}

	nodes.insert(child_id, child_node);

	Ok((child_id, child_node))
}

impl Layout {
	pub fn add_child(
		&mut self,
		parent_widget_id: WidgetID,
		widget: WidgetState,
		style: taffy::Style,
	) -> anyhow::Result<(WidgetID, taffy::NodeId)> {
		let parent_node = *self.state.nodes.get(parent_widget_id).unwrap();

		self.needs_redraw = true;

		add_child_internal(
			&mut self.state.tree,
			&mut self.state.widgets,
			&mut self.state.nodes,
			Some(parent_node),
			widget,
			style,
		)
	}

	fn process_pending_components(&mut self) -> anyhow::Result<()> {
		let mut alterables = EventAlterables::default();

		while let Some(c) = self.components_to_init.pop_front() {
			c.init(&mut InitData {
				state: &self.state,
				alterables: &mut alterables,
			});
		}

		self.process_alterables(alterables)?;

		Ok(())
	}

	pub fn defer_component_init(&mut self, component: Rc<dyn Component>) {
		self.components_to_init.push_back(component);
	}

	fn push_event_children<U1, U2>(
		&self,
		listeners: &EventListenerCollection<U1, U2>,
		parent_node_id: taffy::NodeId,
		event: &event::Event,
		alterables: &mut EventAlterables,
		user_data: &mut (&mut U1, &mut U2),
	) -> anyhow::Result<()> {
		for child_id in self.state.tree.child_ids(parent_node_id) {
			self.push_event_widget(listeners, child_id, event, alterables, user_data)?;
		}

		Ok(())
	}

	fn push_event_widget<U1, U2>(
		&self,
		listeners: &EventListenerCollection<U1, U2>,
		node_id: taffy::NodeId,
		event: &event::Event,
		alterables: &mut EventAlterables,
		user_data: &mut (&mut U1, &mut U2),
	) -> anyhow::Result<()> {
		let l = self.state.tree.layout(node_id)?;
		let Some(widget_id) = self.state.tree.get_node_context(node_id).cloned() else {
			anyhow::bail!("invalid widget ID");
		};

		let style = self.state.tree.style(node_id)?;

		let Some(widget) = self.state.widgets.get(widget_id) else {
			debug_assert!(false);
			anyhow::bail!("invalid widget");
		};

		let mut widget = widget.lock();

		let transform = Transform {
			pos: Vec2::new(l.location.x, l.location.y),
			dim: Vec2::new(l.size.width, l.size.height),
			transform: glam::Mat4::IDENTITY, // TODO: event transformations? Not needed for now
		};

		alterables.transform_stack.push(transform);

		let mut iter_children = true;

		let mut params = EventParams {
			state: &self.state,
			layout: l,
			alterables,
			node_id,
			style,
		};

		let listeners_vec = listeners.get(widget_id);

		match widget.process_event(
			widget_id,
			listeners_vec,
			node_id,
			event,
			user_data,
			&mut params,
		) {
			widget::EventResult::Pass => {
				// go on
			}
			widget::EventResult::Consumed => {
				iter_children = false;
			}
			widget::EventResult::Outside => {
				iter_children = false;
			}
			widget::EventResult::Unused => {
				iter_children = false;
			}
		}

		drop(widget); // free mutex

		if iter_children {
			self.push_event_children(listeners, node_id, event, alterables, user_data)?;
		}

		alterables.transform_stack.pop();

		Ok(())
	}

	pub fn check_toggle_needs_redraw(&mut self) -> bool {
		if self.needs_redraw {
			self.needs_redraw = false;
			true
		} else {
			false
		}
	}

	pub fn check_toggle_haptics_triggered(&mut self) -> bool {
		if self.haptics_triggered {
			self.haptics_triggered = false;
			true
		} else {
			false
		}
	}

	pub fn push_event<U1, U2>(
		&mut self,
		listeners: &mut EventListenerCollection<U1, U2>,
		event: &event::Event,
		mut user_data: (&mut U1, &mut U2),
	) -> anyhow::Result<()> {
		let mut alterables = EventAlterables::default();

		self.push_event_widget(
			listeners,
			self.root_node,
			event,
			&mut alterables,
			&mut user_data,
		)?;

		self.process_alterables(alterables)?;

		listeners.gc();

		Ok(())
	}

	pub fn new(globals: WguiGlobals) -> anyhow::Result<Self> {
		let mut state = LayoutState {
			tree: TaffyTree::new(),
			widgets: WidgetMap::new(),
			nodes: WidgetNodeMap::default(),
			globals,
			alterables: EventAlterables::default(),
		};

		let (root_widget, root_node) = add_child_internal(
			&mut state.tree,
			&mut state.widgets,
			&mut state.nodes,
			None, // no parent
			Div::create()?,
			taffy::Style {
				size: taffy::Size::auto(),
				..Default::default()
			},
		)?;

		Ok(Self {
			state,
			prev_size: Vec2::default(),
			content_size: Vec2::default(),
			root_node,
			root_widget,
			needs_redraw: true,
			haptics_triggered: false,
			animations: Animations::default(),
			components_to_init: VecDeque::new(),
		})
	}

	pub fn update(&mut self, size: Vec2, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut alterables = EventAlterables::default();

		self
			.animations
			.process(&self.state, &mut alterables, timestep_alpha);

		self.process_alterables(alterables)?;

		if self.state.tree.dirty(self.root_node)? || self.prev_size != size {
			self.needs_redraw = true;
			log::debug!("re-computing layout, size {}x{}", size.x, size.y);
			self.prev_size = size;
			self.state.tree.compute_layout_with_measure(
				self.root_node,
				taffy::Size {
					width: taffy::AvailableSpace::Definite(size.x),
					height: taffy::AvailableSpace::Definite(size.y),
				},
				|known_dimensions, available_space, _node_id, node_context, _style| {
					if let taffy::Size {
						width: Some(width),
						height: Some(height),
					} = known_dimensions
					{
						return taffy::Size { width, height };
					}

					match node_context {
						None => taffy::Size::ZERO,
						Some(h) => {
							if let Some(w) = self.state.widgets.get(*h) {
								w.lock().obj.measure(known_dimensions, available_space)
							} else {
								taffy::Size::ZERO
							}
						}
					}
				},
			)?;
			let root_size = self.state.tree.layout(self.root_node).unwrap().size;
			log::debug!(
				"content size {:.0}x{:.0} → {:.0}x{:.0}",
				self.content_size.x,
				self.content_size.y,
				root_size.width,
				root_size.height
			);
			self.content_size = vec2(root_size.width, root_size.height);
		}
		Ok(())
	}

	pub fn tick(&mut self) -> anyhow::Result<()> {
		let mut alterables = EventAlterables::default();
		self.animations.tick(&self.state, &mut alterables);
		self.process_pending_components()?;
		self.process_alterables(alterables)?;
		let state_alterables = std::mem::take(&mut self.state.alterables);
		self.process_alterables(state_alterables)?;

		Ok(())
	}

	fn process_alterables(&mut self, alterables: EventAlterables) -> anyhow::Result<()> {
		for node in alterables.dirty_nodes {
			self.state.tree.mark_dirty(node)?;
		}

		if alterables.needs_redraw {
			self.needs_redraw = true;
		}

		if alterables.trigger_haptics {
			self.haptics_triggered = true;
		}

		if !alterables.animations.is_empty() {
			self.needs_redraw = true;
			for anim in alterables.animations {
				self.animations.add(anim);
			}
		}

		for request in alterables.style_set_requests {
			if let Err(e) = self.state.tree.set_style(request.0, request.1) {
				log::error!(
					"failed to set style for taffy widget ID {:?}: {:?}",
					request.0,
					e
				);
			}
		}

		Ok(())
	}
}
