use std::sync::{Arc, Mutex};

use crate::{
	animation::{self, Animations},
	assets::AssetProvider,
	event::{self, EventListener},
	transform_stack::{Transform, TransformStack},
	widget::{self, EventParams, WidgetState, div::Div},
};

use glam::{Vec2, vec2};
use slotmap::{HopSlotMap, SecondaryMap, new_key_type};
use taffy::{TaffyTree, TraversePartialTree};

new_key_type! {
	pub struct WidgetID;
}

pub type BoxWidget = Arc<Mutex<WidgetState>>;
pub type WidgetMap = HopSlotMap<WidgetID, BoxWidget>;
pub type WidgetNodeMap = SecondaryMap<WidgetID, taffy::NodeId>;

struct PushEventState<'a> {
	pub animations: &'a mut Vec<animation::Animation>,
	pub transform_stack: &'a mut TransformStack,
	pub needs_redraw: bool,
	pub trigger_haptics: bool,
}

pub struct Layout {
	pub tree: TaffyTree<WidgetID>,

	pub assets: Box<dyn AssetProvider>,

	pub widget_map: WidgetMap,
	pub widget_node_map: WidgetNodeMap,

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
	widget_map: &mut WidgetMap,
	widget_node_map: &mut WidgetNodeMap,
	parent_node: Option<taffy::NodeId>,
	widget: WidgetState,
	style: taffy::Style,
) -> anyhow::Result<(WidgetID, taffy::NodeId)> {
	#[allow(clippy::arc_with_non_send_sync)]
	let child_id = widget_map.insert(Arc::new(Mutex::new(widget)));
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
		let parent_node = *self.widget_node_map.get(parent_widget_id).unwrap();

		self.needs_redraw = true;

		add_child_internal(
			&mut self.tree,
			&mut self.widget_map,
			&mut self.widget_node_map,
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

		let Some(widget) = self.widget_map.get(widget_id) else {
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
				widgets: &self.widget_map,
				tree: &self.tree,
				animations: state.animations,
				needs_redraw: &mut state.needs_redraw,
				trigger_haptics: &mut state.trigger_haptics,
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

	pub fn check_toggle_haptics_triggered(&mut self) -> bool {
		if self.haptics_triggered {
			self.haptics_triggered = false;
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
			transform_stack: &mut transform_stack,
			animations: &mut animations_to_add,
			needs_redraw: false,
			trigger_haptics: false,
		};

		self.push_event_widget(&mut state, self.root_node, event, &mut dirty_nodes)?;

		for node in dirty_nodes {
			self.tree.mark_dirty(node)?;
		}

		if state.trigger_haptics {
			self.haptics_triggered = true;
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
		let mut widget_node_map = WidgetNodeMap::default();
		let mut widget_map = HopSlotMap::with_key();

		let (root_widget, root_node) = add_child_internal(
			&mut tree,
			&mut widget_map,
			&mut widget_node_map,
			None, // no parent
			Div::create()?,
			taffy::Style {
				size: taffy::Size::auto(),
				..Default::default()
			},
		)?;

		Ok(Self {
			tree,
			prev_size: Vec2::default(),
			content_size: Vec2::default(),
			root_node,
			root_widget,
			widget_node_map,
			widget_map,
			needs_redraw: true,
			haptics_triggered: false,
			animations: Animations::default(),
			assets,
		})
	}

	pub fn update(&mut self, size: Vec2, timestep_alpha: f32) -> anyhow::Result<()> {
		let mut dirty_nodes = Vec::new();

		self.animations.process(
			&self.widget_map,
			&self.widget_node_map,
			&self.tree,
			&mut dirty_nodes,
			timestep_alpha,
			&mut self.needs_redraw,
		);

		for node in dirty_nodes {
			self.tree.mark_dirty(node)?;
		}

		if self.tree.dirty(self.root_node)? || self.prev_size != size {
			self.needs_redraw = true;
			log::debug!("re-computing layout, size {}x{}", size.x, size.y);
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
							if let Some(w) = self.widget_map.get(*h) {
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
			let root_size = self.tree.layout(self.root_node).unwrap().size;
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
		let mut dirty_nodes = Vec::new();

		self.animations.tick(
			&self.widget_map,
			&self.widget_node_map,
			&self.tree,
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
		let Some(widget) = self.widget_map.get(widget_id) else {
			debug_assert!(false);
			return;
		};
		widget.lock().unwrap().add_event_listener(listener);
	}
}
