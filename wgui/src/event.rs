use std::{cell::RefCell, rc::Rc};

use glam::Vec2;
use slotmap::SecondaryMap;

use crate::{
	animation::{self, Animation},
	layout::{WidgetID, WidgetMap, WidgetNodeMap},
	transform_stack::{Transform, TransformStack},
	widget::{WidgetData, WidgetObj},
};

#[derive(Debug, Clone, Copy)]
pub enum MouseButtonIndex {
	Left,
	Right,
	Middle,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseButton {
	pub index: MouseButtonIndex,
	pub pos: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct MousePosition {
	pub pos: Vec2,
}

pub struct MouseDownEvent {
	pub pos: Vec2,
	pub index: MouseButtonIndex,
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
	pub index: MouseButtonIndex,
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

pub struct EventRefs<'a> {
	pub widgets: &'a WidgetMap,
	pub nodes: &'a WidgetNodeMap,
	pub tree: &'a taffy::tree::TaffyTree<WidgetID>,
}

#[derive(Default)]
pub struct EventAlterables {
	pub dirty_nodes: Vec<taffy::NodeId>,
	pub style_set_requests: Vec<(taffy::NodeId, taffy::Style)>,
	pub animations: Vec<animation::Animation>,
	pub transform_stack: TransformStack,
	pub needs_redraw: bool,
	pub trigger_haptics: bool,
}

impl EventAlterables {
	pub fn mark_redraw(&mut self) {
		self.needs_redraw = true;
	}

	pub fn set_style(&mut self, node_id: taffy::NodeId, style: taffy::Style) {
		self.style_set_requests.push((node_id, style));
	}

	pub fn mark_dirty(&mut self, node_id: taffy::NodeId) {
		self.dirty_nodes.push(node_id);
	}

	pub fn trigger_haptics(&mut self) {
		self.trigger_haptics = true;
	}

	pub fn animate(&mut self, animation: Animation) {
		self.animations.push(animation);
	}
}

pub struct CallbackDataCommon<'a> {
	pub refs: &'a EventRefs<'a>,
	pub alterables: &'a mut EventAlterables,
}

pub struct CallbackData<'a> {
	pub obj: &'a mut dyn WidgetObj,
	pub widget_data: &'a mut WidgetData,
	pub widget_id: WidgetID,
	pub node_id: taffy::NodeId,
	pub metadata: CallbackMetadata,
}

pub enum CallbackMetadata {
	None,
	MouseButton(MouseButton),
	MousePosition(MousePosition),
	Custom(usize),
}

impl CallbackMetadata {
	// helper function
	pub fn get_mouse_pos_absolute(&self) -> Option<Vec2> {
		match *self {
			CallbackMetadata::None => None,
			CallbackMetadata::MouseButton(b) => Some(b.pos),
			CallbackMetadata::MousePosition(b) => Some(b.pos),
			CallbackMetadata::Custom(_) => None,
		}
	}

	pub fn get_mouse_pos_relative(&self, transform_stack: &TransformStack) -> Option<Vec2> {
		let mouse_pos_abs = self.get_mouse_pos_absolute()?;
		Some(mouse_pos_abs - transform_stack.get_pos())
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventListenerKind {
	MousePress,
	MouseRelease,
	MouseEnter,
	MouseMotion,
	MouseLeave,
	InternalStateChange,
}

pub type EventCallback<U1, U2> =
	Box<dyn Fn(&mut CallbackDataCommon, &mut CallbackData, &mut U1, &mut U2)>;

//for ref-counting
pub struct ListenerHandle {
	needs_gc: Rc<RefCell<bool>>, // this will be set to true on destructor
}

#[derive(Default)]
pub struct ListenerHandleVec(Vec<Rc<ListenerHandle>>);

impl ListenerHandleVec {
	pub fn push(&mut self, handle: Rc<ListenerHandle>) {
		self.0.push(handle);
	}
}

impl Drop for ListenerHandle {
	fn drop(&mut self) {
		*self.needs_gc.borrow_mut() = true;
	}
}

pub struct EventListener<U1, U2> {
	pub kind: EventListenerKind,
	pub callback: EventCallback<U1, U2>,
	pub handle: std::rc::Weak<ListenerHandle>,
}

impl<U1, U2> EventListener<U1, U2> {
	pub fn callback_for_kind(
		&self,
		kind: EventListenerKind,
	) -> Option<&impl Fn(&mut CallbackDataCommon, &mut CallbackData, &mut U1, &mut U2)> {
		if self.kind == kind {
			Some(&self.callback)
		} else {
			None
		}
	}
}

#[derive(Default)]
pub struct EventListenerVec<U1, U2>(Vec<EventListener<U1, U2>>);

impl<U1, U2> EventListenerVec<U1, U2> {
	pub fn iter(&self) -> impl Iterator<Item = &EventListener<U1, U2>> {
		self.0.iter().filter(|p| p.handle.strong_count() > 0)
	}
}

pub struct EventListenerCollection<U1, U2> {
	map: SecondaryMap<WidgetID, EventListenerVec<U1, U2>>,
	needs_gc: Rc<RefCell<bool>>,
}

// derive only works if generics also implement Default
impl<U1, U2> Default for EventListenerCollection<U1, U2> {
	fn default() -> Self {
		Self {
			map: SecondaryMap::default(),
			needs_gc: Rc::new(RefCell::new(false)),
		}
	}
}

impl<U1, U2> EventListenerCollection<U1, U2> {
	pub fn register(
		&mut self,
		listener_handles: &mut ListenerHandleVec,
		widget_id: WidgetID,
		kind: EventListenerKind,
		callback: EventCallback<U1, U2>,
	) {
		let res = self.add_single(widget_id, kind, callback);
		listener_handles.push(res);
	}

	pub fn add_single(
		&mut self,
		widget_id: WidgetID,
		kind: EventListenerKind,
		callback: EventCallback<U1, U2>,
	) -> Rc<ListenerHandle> {
		let handle = Rc::new(ListenerHandle {
			needs_gc: self.needs_gc.clone(),
		});

		let new_item = EventListener {
			kind,
			callback,
			handle: Rc::downgrade(&handle),
		};
		if let Some(vec) = self.map.get_mut(widget_id) {
			vec.0.push(new_item);
		} else {
			self.map.insert(widget_id, EventListenerVec(vec![new_item]));
		}

		handle
	}

	// clean-up expired events
	pub fn gc(&mut self) {
		let mut needs_gc = self.needs_gc.borrow_mut();
		if !*needs_gc {
			return;
		}

		*needs_gc = false;

		let mut count = 0;

		for (_id, vec) in self.map.iter_mut() {
			vec.0.retain(|listener| {
				if listener.handle.strong_count() != 0 {
					true
				} else {
					count += 1;
					false
				}
			});
		}

		self.map.retain(|_k, v| !v.0.is_empty());

		log::debug!("EventListenerCollection: cleaned-up {count} expired events");
	}

	pub fn get(&self, widget_id: WidgetID) -> Option<&EventListenerVec<U1, U2>> {
		self.map.get(widget_id)
	}
}
