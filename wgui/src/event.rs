use glam::Vec2;

use crate::{
	animation,
	layout::{WidgetID, WidgetMap},
	transform_stack::Transform,
	widget::{WidgetData, WidgetObj},
};

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
	Left,
	Right,
	Middle,
}

pub struct MouseDownEvent {
	pub pos: Vec2,
	pub button: MouseButton,
	pub device: usize,
}

pub struct MouseLeaveEvent {
	pub device: usize,
}

pub struct MouseMotionEvent {
	pub pos: Vec2,
	pub device: usize,
}

pub struct MouseUpEvent {
	pub pos: Vec2,
	pub button: MouseButton,
	pub device: usize,
}

pub struct MouseWheelEvent {
	pub pos: Vec2,
	pub shift: Vec2,
	pub device: usize,
}

pub struct InternalStateChangeEvent {
	pub metadata: usize,
}

pub enum Event {
	InternalStateChange(InternalStateChangeEvent),
	MouseDown(MouseDownEvent),
	MouseLeave(MouseLeaveEvent),
	MouseMotion(MouseMotionEvent),
	MouseUp(MouseUpEvent),
	MouseWheel(MouseWheelEvent),
}

impl Event {
	fn test_transform_pos(&self, transform: &Transform, pos: &Vec2) -> bool {
		pos.x >= transform.pos.x
			&& pos.x < transform.pos.x + transform.dim.x
			&& pos.y >= transform.pos.y
			&& pos.y < transform.pos.y + transform.dim.y
	}

	pub fn test_mouse_within_transform(&self, transform: &Transform) -> bool {
		match self {
			Event::MouseDown(evt) => self.test_transform_pos(transform, &evt.pos),
			Event::MouseMotion(evt) => self.test_transform_pos(transform, &evt.pos),
			Event::MouseUp(evt) => self.test_transform_pos(transform, &evt.pos),
			Event::MouseWheel(evt) => self.test_transform_pos(transform, &evt.pos),
			_ => false,
		}
	}
}

pub trait WidgetCallback<'a> {
	fn call_on_widget<WIDGET, FUNC>(&self, widget_id: WidgetID, func: FUNC)
	where
		WIDGET: WidgetObj,
		FUNC: FnOnce(&mut WIDGET),
	{
		let Some(widget) = self.get_widgets().get(widget_id) else {
			debug_assert!(false);
			return;
		};

		let mut lock = widget.lock().unwrap();
		let m = lock.obj.get_as_mut::<WIDGET>();

		func(m);
	}

	fn get_widgets(&self) -> &'a WidgetMap;
	fn mark_redraw(&mut self);
	fn mark_dirty(&mut self, node_id: taffy::NodeId);
}

pub struct CallbackData<'a> {
	pub obj: &'a mut dyn WidgetObj,
	pub widget_data: &'a mut WidgetData,
	pub animations: &'a mut Vec<animation::Animation>,
	pub widgets: &'a WidgetMap,
	pub widget_id: WidgetID,
	pub node_id: taffy::NodeId,
	pub dirty_nodes: &'a mut Vec<taffy::NodeId>,
	pub needs_redraw: bool,
	pub trigger_haptics: bool,
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

pub type MouseEnterCallback = Box<dyn Fn(&mut CallbackData, ())>;
pub type MouseLeaveCallback = Box<dyn Fn(&mut CallbackData, ())>;
pub type MousePressCallback = Box<dyn Fn(&mut CallbackData, MouseButton)>;
pub type MouseReleaseCallback = Box<dyn Fn(&mut CallbackData, MouseButton)>;
pub type InternalStateChangeCallback = Box<dyn Fn(&mut CallbackData, usize)>;

pub enum EventListener {
	MouseEnter(MouseEnterCallback),
	MouseLeave(MouseLeaveCallback),
	MousePress(MousePressCallback),
	MouseRelease(MouseReleaseCallback),
	InternalStateChange(InternalStateChangeCallback),
}