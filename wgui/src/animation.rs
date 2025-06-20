use glam::{FloatExt, Vec2};

use crate::{
	event::WidgetCallback,
	layout::{WidgetID, WidgetMap, WidgetNodeMap},
	widget::{WidgetData, WidgetObj},
};

pub enum AnimationEasing {
	Linear,
	InQuad,
	OutQuad,
	OutBack,
	InBack,
}

impl AnimationEasing {
	fn interpolate(&self, x: f32) -> f32 {
		match self {
			AnimationEasing::Linear => x,
			AnimationEasing::InQuad => x * x,
			AnimationEasing::OutQuad => 1.0 - (1.0 - x) * (1.0 - x),
			AnimationEasing::OutBack => {
				let a = 1.7;
				let b = a + 1.0;
				1.0 + b * (x - 1.0).powf(3.0) + a * (x - 1.0).powf(2.0)
			}
			AnimationEasing::InBack => {
				let a = 1.7;
				let b = a + 1.0;
				b * x.powf(3.0) - a * x.powf(2.0)
			}
		}
	}
}

pub struct CallbackData<'a> {
	pub obj: &'a mut dyn WidgetObj,
	pub data: &'a mut WidgetData,
	pub widgets: &'a WidgetMap,
	pub widget_id: WidgetID,
	pub widget_size: Vec2,
	pub pos: f32, // 0.0 (start of animation) - 1.0 (end of animation)
	pub needs_redraw: bool,
	pub dirty_nodes: &'a mut Vec<taffy::NodeId>,
}

impl<'a> WidgetCallback<'a> for CallbackData<'a> {
	fn get_widgets(&self) -> &'a WidgetMap {
		self.widgets
	}

	fn mark_redraw(&mut self) {
		self.needs_redraw = true;
	}

	fn mark_dirty(&mut self, node_id: taffy::NodeId) {
		self.dirty_nodes.push(node_id);
	}
}

pub struct Animation {
	target_widget: WidgetID,

	animation_id: u32,
	ticks_remaining: u32,
	ticks_duration: u32,

	easing: AnimationEasing,

	pos: f32,
	pos_prev: f32,
	last_tick: bool,

	callback: Box<dyn Fn(&mut CallbackData)>,
}

#[derive(Default)]
struct CallResult {
	needs_redraw: bool,
}

impl Animation {
	pub fn new(
		target_widget: WidgetID,
		ticks: u32,
		easing: AnimationEasing,
		callback: Box<dyn Fn(&mut CallbackData)>,
	) -> Self {
		Animation::new_ex(target_widget, 0, ticks, easing, callback)
	}

	pub fn new_ex(
		target_widget: WidgetID,
		animation_id: u32,
		ticks: u32,
		easing: AnimationEasing,
		callback: Box<dyn Fn(&mut CallbackData)>,
	) -> Self {
		Self {
			target_widget,
			animation_id,
			callback,
			easing,
			ticks_duration: ticks,
			ticks_remaining: ticks,
			last_tick: false,
			pos: 0.0,
			pos_prev: 0.0,
		}
	}

	fn call(
		&self,
		widget_map: &WidgetMap,
		widget_node_map: &WidgetNodeMap,
		tree: &taffy::tree::TaffyTree<WidgetID>,
		dirty_nodes: &mut Vec<taffy::NodeId>,
		pos: f32,
	) -> CallResult {
		let mut res = CallResult::default();

		let Some(widget) = widget_map.get(self.target_widget).cloned() else {
			return res; // failed
		};

		let widget_node = widget_node_map.get(self.target_widget);
		let layout = tree.layout(widget_node).unwrap(); // should always succeed

		let mut widget = widget.lock().unwrap();

		let (data, obj) = widget.get_data_obj_mut();

		let data = &mut CallbackData {
			widget_id: self.target_widget,
			dirty_nodes,
			widgets: widget_map,
			widget_size: Vec2::new(layout.size.width, layout.size.height),
			obj,
			data,
			pos,
			needs_redraw: false,
		};

		(self.callback)(data);

		if data.needs_redraw {
			res.needs_redraw = true;
		}

		res
	}
}

#[derive(Default)]
pub struct Animations {
	running_animations: Vec<Animation>,
}

impl Animations {
	pub fn tick(
		&mut self,
		widget_map: &WidgetMap,
		widget_node_map: &WidgetNodeMap,
		tree: &taffy::tree::TaffyTree<WidgetID>,
		dirty_nodes: &mut Vec<taffy::NodeId>,
		needs_redraw: &mut bool,
	) {
		for anim in &mut self.running_animations {
			let x = 1.0 - (anim.ticks_remaining as f32 / anim.ticks_duration as f32);
			let pos = if anim.ticks_remaining > 0 {
				anim.easing.interpolate(x)
			} else {
				anim.last_tick = true;
				1.0
			};

			anim.pos_prev = anim.pos;
			anim.pos = pos;

			let res = anim.call(widget_map, widget_node_map, tree, dirty_nodes, 1.0);

			if anim.last_tick || res.needs_redraw {
				*needs_redraw = true;
			}

			anim.ticks_remaining -= 1;
		}

		self
			.running_animations
			.retain(|anim| anim.ticks_remaining > 0);
	}

	pub fn process(
		&mut self,
		widget_map: &WidgetMap,
		widget_node_map: &WidgetNodeMap,
		tree: &taffy::tree::TaffyTree<WidgetID>,
		dirty_nodes: &mut Vec<taffy::NodeId>,
		alpha: f32,
		needs_redraw: &mut bool,
	) {
		for anim in &mut self.running_animations {
			let pos = anim.pos_prev.lerp(anim.pos, alpha);
			let res = anim.call(widget_map, widget_node_map, tree, dirty_nodes, pos);

			if res.needs_redraw {
				*needs_redraw = true;
			}
		}
	}

	pub fn add(&mut self, anim: Animation) {
		// prevent running two animations at once
		self.stop_by_widget(anim.target_widget, Some(anim.animation_id));
		self.running_animations.push(anim);
	}

	pub fn stop_by_widget(&mut self, widget_id: WidgetID, animation_id: Option<u32>) {
		self.running_animations.retain(|anim| {
			if let Some(animation_id) = &animation_id {
				if anim.target_widget == widget_id {
					anim.animation_id != *animation_id
				} else {
					true
				}
			} else {
				anim.target_widget != widget_id
			}
		});
	}
}
