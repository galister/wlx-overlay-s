use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use crate::{
	animation::{self, Animations},
	assets::AssetProvider,
	event::{self, EventListener},
	transform_stack::{Transform, TransformStack},
	widget::{self, EventParams, WidgetState, div::Div},
};

use glam::Vec2;
use slotmap::HopSlotMap;
use taffy::{TaffyTree, TraversePartialTree};

pub type WidgetID = slotmap::DefaultKey;
pub type BoxWidget = Arc<Mutex<WidgetState>>;
pub type WidgetMap = HopSlotMap<slotmap::DefaultKey, BoxWidget>;

struct PushEventState<'a> {
	pub needs_redraw: bool,
	pub animations: &'a mut Vec<animation::Animation>,
	pub transform_stack: &'a mut TransformStack,
}

pub struct Layout {
	pub tree: TaffyTree<WidgetID>,

	pub assets: Box<dyn AssetProvider>,

	pub widget_states: WidgetMap,
	pub widget_node_map: HashMap<WidgetID, taffy::NodeId>,

	pub root_widget: WidgetID,
	pub root_node: taffy::NodeId,

	pub prev_size: Vec2,

	pub needs_redraw: bool,

	pub animations: Animations,
}

fn add_child_internal(
	tree: &mut taffy::TaffyTree<WidgetID>,
	widget_node_map: &mut HashMap<WidgetID, taffy::NodeId>,
	vec: &mut WidgetMap,
	parent_node: Option<taffy::NodeId>,
	widget: WidgetState,
	style: taffy::Style,
) -> anyhow::Result<(WidgetID, taffy::NodeId)> {
	#[allow(clippy::arc_with_non_send_sync)]
	let child_id = vec.insert(Arc::new(Mutex::new(widget)));
	let child_node = tree.new_leaf_with_context(style, child_id)?;

	if let Some(parent_node) = parent_node {
		tree.add_child(parent_node, child_node)?;
	}

	widget_node_map.insert(child_id, child_node);

	Ok((child_id, child_node))
}

impl Layout {
	pub fn add_child(
		&mut self,
		parent_widget_id: WidgetID,
		widget: WidgetState,
		style: taffy::Style,
	) -> anyhow::Result<(WidgetID, taffy::NodeId)> {
		let Some(parent_node) = self.widget_node_map.get(&parent_widget_id).cloned() else {
			anyhow::bail!("invalid parent widget");
		};

		self.needs_redraw = true;

		add_child_internal(
			&mut self.tree,
			&mut self.widget_node_map,
			&mut self.widget_states,
			Some(parent_node),
			widget,
			style,
		)
	}

	fn push_event_children(
		&self,
		parent_node_id: taffy::NodeId,
		state: &mut PushEventState,
		event: &event::Event,
		dirty_nodes: &mut Vec<taffy::NodeId>,
	) -> anyhow::Result<()> {
		for child_id in self.tree.child_ids(parent_node_id) {
			self.push_event_widget(state, child_id, event, dirty_nodes)?;
		}

		Ok(())
	}

	fn push_event_widget(
		&self,
		state: &mut PushEventState,
		node_id: taffy::NodeId,
		event: &event::Event,
		dirty_nodes: &mut Vec<taffy::NodeId>,
	) -> anyhow::Result<()> {
		let l = self.tree.layout(node_id)?;
		let Some(widget_id) = self.tree.get_node_context(node_id).cloned() else {
			anyhow::bail!("invalid widget ID");
		};

		let style = self.tree.style(node_id)?;

		let Some(widget) = self.widget_states.get(widget_id) else {
			debug_assert!(false);
			anyhow::bail!("invalid widget");
		};

		let mut widget = widget.lock().unwrap();

		let transform = Transform {
			pos: Vec2::new(l.location.x, l.location.y),
			dim: Vec2::new(l.size.width, l.size.height),
			transform: glam::Mat4::IDENTITY, // TODO: event transformations? Not needed for now
		};

		state.transform_stack.push(transform);

		let mut iter_children = true;

		match widget.process_event(
			widget_id,
			node_id,
			event,
			&mut EventParams {
				transform_stack: state.transform_stack,
				widgets: &self.widget_states,
				tree: &self.tree,
				animations: state.animations,
				needs_redraw: &mut state.needs_redraw,
				node_id,
				style,
				taffy_layout: l,
				dirty_nodes,
			},
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
		}

		drop(widget); // free mutex

		if iter_children {
			self.push_event_children(node_id, state, event, dirty_nodes)?;
		}

		state.transform_stack.pop();

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

	pub fn push_event(&mut self, event: &event::Event) -> anyhow::Result<()> {
		let mut transform_stack = TransformStack::new();
		let mut animations_to_add = Vec::<animation::Animation>::new();
		let mut dirty_nodes = Vec::new();

		let mut state = PushEventState {
			needs_redraw: false,
			transform_stack: &mut transform_stack,
			animations: &mut animations_to_add,
		};

		self.push_event_widget(&mut state, self.root_node, event, &mut dirty_nodes)?;

		for node in dirty_nodes {
			self.tree.mark_dirty(node)?;
		}

		if state.needs_redraw {
			self.needs_redraw = true;
		}

		if !animations_to_add.is_empty() {
			self.needs_redraw = true;
			for anim in animations_to_add {
				self.animations.add(anim);
			}
		}

		Ok(())
	}

	pub fn new(assets: Box<dyn AssetProvider>) -> anyhow::Result<Self> {
		let mut tree = TaffyTree::new();
		let mut widget_node_map = HashMap::new();
		let mut widget_states = HopSlotMap::new();

		let (root_widget, root_node) = add_child_internal(
			&mut tree,
			&mut widget_node_map,
			&mut widget_states,
			None, // no parent
			Div::create()?,
			taffy::Style {
				size: taffy::Size::percent(1.0),
				..Default::default()
			},
		)?;

		Ok(Self {
			tree,
			prev_size: Vec2::default(),
			root_node,
			root_widget,
			widget_node_map,
			widget_states,
			needs_redraw: true,
			animations: Animations::default(),
			assets,
		})
	}

	pub fn update(&mut self, size: Vec2, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut dirty_nodes = Vec::new();

		self.animations.process(
			&self.widget_states,
			&mut dirty_nodes,
			timestep_alpha,
			&mut self.needs_redraw,
		);

		for node in dirty_nodes {
			self.tree.mark_dirty(node)?;
		}

		if self.tree.dirty(self.root_node)? || self.prev_size != size {
			self.needs_redraw = true;
			println!("re-computing layout, size {}x{}", size.x, size.y);
			self.prev_size = size;
			self.tree.compute_layout_with_measure(
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
							if let Some(w) = self.widget_states.get(*h) {
								w.lock()
									.unwrap()
									.obj
									.measure(known_dimensions, available_space)
							} else {
								taffy::Size::ZERO
							}
						}
					}
				},
			)?;
		}
		Ok(())
	}

	pub fn tick(&mut self) -> anyhow::Result<()> {
		let mut dirty_nodes = Vec::new();

		self.animations.tick(
			&self.widget_states,
			&mut dirty_nodes,
			&mut self.needs_redraw,
		);

		for node in dirty_nodes {
			self.tree.mark_dirty(node)?;
		}

		Ok(())
	}

	// helper function
	pub fn add_event_listener(&self, widget_id: WidgetID, listener: EventListener) {
		let Some(widget) = self.widget_states.get(widget_id) else {
			debug_assert!(false);
			return;
		};
		widget.lock().unwrap().add_event_listener(listener);
	}
}