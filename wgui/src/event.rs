use std::{
	any::{Any, TypeId},
	cell::RefMut,
	collections::HashSet,
};

use glam::Vec2;
use slotmap::{DenseSlotMap, new_key_type};

use crate::{
	animation::{self, Animation},
	i18n::I18n,
	layout::{LayoutState, LayoutTask, WidgetID},
	stack::{ScissorStack, Transform, TransformStack},
	widget::{EventResult, WidgetData, WidgetObj},
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
	fn test_transform_pos(transform: &Transform, pos: Vec2) -> bool {
		pos.x >= transform.abs_pos.x
			&& pos.x < transform.abs_pos.x + transform.visual_dim.x
			&& pos.y >= transform.abs_pos.y
			&& pos.y < transform.abs_pos.y + transform.visual_dim.y
	}

	pub fn test_mouse_within_transform(&self, transform: &Transform) -> bool {
		match self {
			Self::MouseDown(evt) => Self::test_transform_pos(transform, evt.pos),
			Self::MouseMotion(evt) => Self::test_transform_pos(transform, evt.pos),
			Self::MouseUp(evt) => Self::test_transform_pos(transform, evt.pos),
			Self::MouseWheel(evt) => Self::test_transform_pos(transform, evt.pos),
			_ => false,
		}
	}
}

// alterables which will be dispatched in the next loop iteration phase
#[derive(Default)]
pub struct EventAlterables {
	pub dirty_nodes: Vec<taffy::NodeId>,
	pub style_set_requests: Vec<(taffy::NodeId, taffy::Style)>,
	pub animations: Vec<animation::Animation>,
	pub widgets_to_tick: HashSet<WidgetID>, // widgets which needs to be ticked in the next `Layout::update()` fn
	pub transform_stack: TransformStack,
	pub scissor_stack: ScissorStack,
	pub tasks: Vec<LayoutTask>,
	pub needs_redraw: bool,
	pub trigger_haptics: bool,
}

// helper functions
impl EventAlterables {
	pub const fn mark_redraw(&mut self) {
		self.needs_redraw = true;
	}

	pub fn set_style(&mut self, node_id: taffy::NodeId, style: taffy::Style) {
		self.style_set_requests.push((node_id, style));
	}

	pub fn mark_dirty(&mut self, node_id: taffy::NodeId) {
		self.dirty_nodes.push(node_id);
	}

	pub fn mark_tick(&mut self, widget_id: WidgetID) {
		self.widgets_to_tick.insert(widget_id);
	}

	pub const fn trigger_haptics(&mut self) {
		self.trigger_haptics = true;
	}

	pub fn animate(&mut self, animation: Animation) {
		self.animations.push(animation);
	}
}

pub struct CallbackDataCommon<'a> {
	pub state: &'a LayoutState,
	pub alterables: &'a mut EventAlterables,
}

impl CallbackDataCommon<'_> {
	pub fn i18n(&self) -> RefMut<'_, I18n> {
		self.state.globals.i18n()
	}

	// helper function
	pub fn mark_widget_dirty(&mut self, id: WidgetID) {
		if let Some(node_id) = self.state.nodes.get(id) {
			self.alterables.mark_dirty(*node_id);
		}
		self.alterables.mark_redraw();
	}
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

	pub const fn get_mouse_pos_absolute(&self) -> Option<Vec2> {
		match *self {
			Self::MouseButton(b) => Some(b.pos),
			Self::MousePosition(b) => Some(b.pos),
			Self::Custom(_) | Self::None => None,
		}
	}

	pub fn get_mouse_pos_relative(&self, transform_stack: &TransformStack) -> Option<Vec2> {
		let mouse_pos_abs = self.get_mouse_pos_absolute()?;
		Some(mouse_pos_abs - transform_stack.get().abs_pos)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventListenerKind {
	MousePress,
	MouseRelease,
	MouseEnter,
	MouseMotion,
	MouseLeave,
	InternalStateChange,
}

pub type EventCallbackInternal = Box<
	dyn for<'a, 'b, 'c, 'd> Fn(
		&'a mut CallbackDataCommon<'b>,
		&'a mut CallbackData<'c>,
		&'d mut dyn Any,
		&'d mut dyn Any,
	) -> anyhow::Result<EventResult>,
>;

pub type EventCallback<U1, U2> = Box<
	dyn for<'a, 'b, 'c, 'd> Fn(
		&'a mut CallbackDataCommon<'b>,
		&'a mut CallbackData<'c>,
		&'d mut U1,
		&'d mut U2,
	) -> anyhow::Result<EventResult>,
>;

new_key_type! {
	pub struct EventListenerID;
}

pub struct EventListener {
	kind: EventListenerKind,
	callback: EventCallbackInternal,
	tid1: Option<TypeId>,
	tid2: Option<TypeId>,
}

impl EventListener {
	pub fn call_with<U1: 'static, U2: 'static>(
		&self,
		common: &mut CallbackDataCommon,
		data: &mut CallbackData,
		user_data: &mut (&mut U1, &mut U2),
	) -> anyhow::Result<EventResult> {
		let a1: &mut (dyn Any + 'static) = if self.tid1.is_none() { &mut () } else { user_data.0 };
		let a2: &mut (dyn Any + 'static) = if self.tid2.is_none() { &mut () } else { user_data.1 };
		(self.callback)(common, data, a1, a2)
	}
}

#[derive(Default)]
pub struct EventListenerCollection {
	inner: DenseSlotMap<EventListenerID, EventListener>,
}

impl EventListenerCollection {
	/// Iterates over event handlers with a matching U type
	pub fn iter_filtered<U1: 'static, U2: 'static>(
		&self,
		kind: EventListenerKind,
	) -> impl Iterator<Item = &EventListener> {
		let tid1 = TypeId::of::<U1>();
		let tid2 = TypeId::of::<U2>();
		self
			.inner
			.values()
			.filter(move |p| p.tid1.is_none_or(|a| a == tid1) && p.tid2.is_none_or(|a| a == tid2) && p.kind == kind)
	}

	pub fn register<U1: 'static, U2: 'static>(
		&mut self,
		kind: EventListenerKind,
		callback: EventCallback<U1, U2>,
	) -> EventListenerID {
		let tid_unit = TypeId::of::<()>();

		let tid1 = TypeId::of::<U1>();
		let tid2 = TypeId::of::<U2>();

		let callback_inner: EventCallbackInternal = Box::new(move |common, data, u1_any, u2_any| {
			if let Some(u1) = u1_any.downcast_mut::<U1>()
				&& let Some(u2) = u2_any.downcast_mut::<U2>()
			{
				callback(common, data, u1, u2)
			} else {
				Ok(EventResult::Pass)
			}
		});

		let new_item = EventListener {
			kind,
			callback: callback_inner,
			tid1: (tid1 != tid_unit).then_some(tid1),
			tid2: (tid2 != tid_unit).then_some(tid2),
		};

		self.inner.insert(new_item)
	}

	pub fn remove(&mut self, event_listener_id: EventListenerID) -> Option<EventListener> {
		self.inner.remove(event_listener_id)
	}
}
