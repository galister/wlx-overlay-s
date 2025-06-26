use glam::Vec2;
use slotmap::SecondaryMap;

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
	pub metadata: CallbackMetadata,
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

pub enum CallbackMetadata {
	None,
	MouseButton(MouseButton),
	Custom(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventListenerKind {
	MousePress,
	MouseRelease,
	MouseEnter,
	MouseLeave,
	InternalStateChange,
}

pub type EventCallback<U1, U2> = Box<dyn Fn(&mut CallbackData, &mut U1, &mut U2)>;

pub struct EventListener<U1, U2> {
	pub kind: EventListenerKind,
	pub callback: EventCallback<U1, U2>,
}

impl<U1, U2> EventListener<U1, U2> {
	pub fn callback_for_kind(
		&self,
		kind: EventListenerKind,
	) -> Option<&impl Fn(&mut CallbackData, &mut U1, &mut U2)> {
		if self.kind == kind {
			Some(&self.callback)
		} else {
			None
		}
	}
}

pub struct EventListenerCollection<U1, U2> {
	map: SecondaryMap<WidgetID, Vec<EventListener<U1, U2>>>,
}

// derive only works if generics also implement Default
impl<U1, U2> Default for EventListenerCollection<U1, U2> {
	fn default() -> Self {
		Self {
			map: SecondaryMap::default(),
		}
	}
}

impl<U1, U2> EventListenerCollection<U1, U2> {
	pub fn add(
		&mut self,
		widget_id: WidgetID,
		kind: EventListenerKind,
		callback: EventCallback<U1, U2>,
	) {
		let new_item = EventListener { kind, callback };
		if let Some(vec) = self.map.get_mut(widget_id) {
			vec.push(new_item);
		} else {
			self.map.insert(widget_id, vec![new_item]);
		}
	}

	pub fn get(&self, widget_id: WidgetID) -> Option<&[EventListener<U1, U2>]> {
		self.map.get(widget_id).map(|v| v.as_slice())
	}
}
