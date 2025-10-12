use glam::Vec2;

use super::drawing::RenderPrimitive;

use crate::{
	any::AnyTrait,
	drawing::{self, PrimitiveExtent},
	event::{
		self, CallbackData, CallbackDataCommon, CallbackMetadata, Event, EventAlterables, EventListenerCollection,
		EventListenerKind::{InternalStateChange, MouseEnter, MouseLeave, MouseMotion, MousePress, MouseRelease},
		MouseWheelEvent,
	},
	layout::{Layout, LayoutState, WidgetID},
	stack::{ScissorStack, TransformStack},
};

pub mod div;
pub mod label;
pub mod rectangle;
pub mod sprite;
pub mod util;

pub struct WidgetData {
	hovered: usize,
	pressed: usize,
	pub scrolling_target: Vec2,   // normalized, 0.0-1.0. Not used in case if overflow != scroll
	pub scrolling_cur: Vec2,      // normalized, used for smooth scrolling animation
	pub scrolling_cur_prev: Vec2, // for motion interpolation while rendering between ticks
	pub transform: glam::Mat4,
}

impl WidgetData {
	pub const fn set_device_pressed(&mut self, device: usize, pressed: bool) -> bool {
		let bit = 1 << device;
		let state_changed;
		if pressed {
			state_changed = self.pressed == 0;
			self.pressed |= bit;
		} else {
			state_changed = self.pressed == bit;
			self.pressed &= !bit;
		}
		state_changed
	}

	pub const fn set_device_hovered(&mut self, device: usize, hovered: bool) -> bool {
		let bit = 1 << device;
		let state_changed;
		if hovered {
			state_changed = self.hovered == 0;
			self.hovered |= bit;
		} else {
			state_changed = self.hovered == bit;
			self.hovered &= !bit;
		}
		state_changed
	}

	pub const fn get_pressed(&self, device: usize) -> bool {
		self.pressed & (1 << device) != 0
	}

	pub const fn get_hovered(&self, device: usize) -> bool {
		self.hovered & (1 << device) != 0
	}

	pub const fn is_pressed(&self) -> bool {
		self.pressed != 0
	}

	pub const fn is_hovered(&self) -> bool {
		self.hovered != 0
	}
}

pub struct WidgetState {
	pub data: WidgetData,
	pub obj: Box<dyn WidgetObj>,
	pub event_listeners: EventListenerCollection,
}

impl WidgetState {
	pub fn get_data_obj_mut(&mut self) -> (&mut WidgetData, &mut dyn WidgetObj) {
		let data = &mut self.data;
		let obj = self.obj.as_mut();
		(data, obj)
	}

	fn new(obj: Box<dyn WidgetObj>) -> Self {
		Self {
			data: WidgetData {
				hovered: 0,
				pressed: 0,
				scrolling_target: Vec2::default(),
				scrolling_cur: Vec2::default(),
				scrolling_cur_prev: Vec2::default(),
				transform: glam::Mat4::IDENTITY,
			},
			obj,
			event_listeners: EventListenerCollection::default(),
		}
	}
}

// global draw params
pub struct DrawState<'a> {
	pub layout: &'a Layout,
	pub primitives: &'a mut Vec<RenderPrimitive>,
	pub transform_stack: &'a mut TransformStack,
	pub scissor_stack: &'a mut ScissorStack,
	pub alterables: &'a mut EventAlterables,
}

// per-widget draw params
pub struct DrawParams<'a> {
	pub node_id: taffy::NodeId,
	pub style: &'a taffy::Style,
	pub taffy_layout: &'a taffy::Layout,
}

pub enum WidgetType {
	Div,
	Label,
	Sprite,
	Rectangle,
}

impl WidgetType {
	pub const fn as_str(&self) -> &str {
		match self {
			WidgetType::Div => "div",
			WidgetType::Label => "label",
			WidgetType::Sprite => "sprite",
			WidgetType::Rectangle => "rectangle",
		}
	}
}

pub trait WidgetObj: AnyTrait {
	// every widget stores their of id for convenience reasons
	fn get_id(&self) -> WidgetID;
	fn set_id(&mut self, id: WidgetID); // always set at insertion
	fn get_type(&self) -> WidgetType;
	fn debug_print(&self) -> String;

	fn draw(&mut self, state: &mut DrawState, params: &DrawParams);

	fn measure(
		&mut self,
		_known_dimensions: taffy::Size<Option<f32>>,
		_available_space: taffy::Size<taffy::AvailableSpace>,
	) -> taffy::Size<f32> {
		taffy::Size::ZERO
	}
}

pub struct EventParams<'a> {
	pub node_id: taffy::NodeId,
	pub style: &'a taffy::Style,
	pub state: &'a LayoutState,
	pub alterables: &'a mut EventAlterables,
	pub layout: &'a taffy::Layout,
}

#[derive(Eq, PartialEq)]
pub enum EventResult {
	Pass,     // widget acknowledged it and allows the event to pass further
	Consumed, // widget triggered an action, do not pass further
}

fn get_scroll_enabled(style: &taffy::Style) -> (bool, bool) {
	(
		style.overflow.x == taffy::Overflow::Scroll,
		style.overflow.y == taffy::Overflow::Scroll,
	)
}

pub struct ScrollbarInfo {
	// total contents size of the currently scrolling widget
	content_size: Vec2,
	// 0.0 - 1.0
	// 1.0: scrollbar handle not visible (inactive)
	handle_size: Vec2,
}

pub fn get_scrollbar_info(l: &taffy::Layout) -> Option<ScrollbarInfo> {
	let overflow = Vec2::new(l.scroll_width(), l.scroll_height());
	if overflow.x == 0.0 && overflow.y == 0.0 {
		return None; // not overflowing
	}

	let content_size = Vec2::new(l.content_size.width, l.content_size.height);
	let handle_size = 1.0 - (overflow / content_size);

	Some(ScrollbarInfo {
		content_size,
		handle_size,
	})
}

impl dyn WidgetObj {
	pub fn get_as<T: 'static>(&self) -> Option<&T> {
		let any = self.as_any();
		any.downcast_ref::<T>()
	}

	pub fn get_as_mut<T: 'static>(&mut self) -> Option<&mut T> {
		let any = self.as_any_mut();
		any.downcast_mut::<T>()
	}
}

macro_rules! call_event {
	($self:ident, $widget_id:ident, $node_id:ident, $params:ident, $kind:ident, $u1:ty, $u2:ty, $user_data:expr, $metadata:expr) => {
		for listener in $self.event_listeners.iter_filtered::<$u1, $u2>($kind) {
			let mut data = CallbackData {
				obj: $self.obj.as_mut(),
				widget_data: &mut $self.data,
				$widget_id,
				$node_id,
				metadata: $metadata,
			};

			let mut common = CallbackDataCommon {
				state: $params.state,
				alterables: $params.alterables,
			};
			let result = listener.call_with(&mut common, &mut data, $user_data)?;
			if result == EventResult::Consumed {
				return Ok(EventResult::Consumed);
			}
		}
	};
}

impl WidgetState {
	pub fn get_scroll_shift_smooth(&self, info: &ScrollbarInfo, l: &taffy::Layout, timestep_alpha: f32) -> (Vec2, bool) {
		let currently_animating = self.data.scrolling_cur != self.data.scrolling_cur_prev;

		let scrolling = self
			.data
			.scrolling_cur_prev
			.lerp(self.data.scrolling_cur, timestep_alpha);

		(
			Vec2::new(
				(info.content_size.x - l.content_box_width()) * scrolling.x,
				(info.content_size.y - l.content_box_height()) * scrolling.y,
			),
			currently_animating,
		)
	}

	pub fn get_scroll_shift_raw(&self, info: &ScrollbarInfo, l: &taffy::Layout) -> Vec2 {
		Vec2::new(
			(info.content_size.x - l.content_box_width()) * self.data.scrolling_target.x,
			(info.content_size.y - l.content_box_height()) * self.data.scrolling_target.y,
		)
	}

	pub fn draw_all(&mut self, state: &mut DrawState, params: &DrawParams) {
		self.obj.draw(state, params);
	}

	pub fn tick(&mut self, this_widget_id: WidgetID, alterables: &mut EventAlterables) {
		let scrolling_cur = &mut self.data.scrolling_cur;
		let scrolling_cur_prev = &mut self.data.scrolling_cur_prev;
		let scrolling_target = &mut self.data.scrolling_target;

		*scrolling_cur_prev = *scrolling_cur;

		if scrolling_cur != scrolling_target {
			// the magic part
			*scrolling_cur = scrolling_cur.lerp(*scrolling_target, 0.2);

			// trigger tick request again
			alterables.mark_tick(this_widget_id);
			alterables.mark_redraw();

			let epsilon = 0.00001;
			if (scrolling_cur.x - scrolling_target.x).abs() < epsilon
				&& (scrolling_cur.y - scrolling_target.y).abs() < epsilon
			{
				log::info!("stopped animating");
				*scrolling_cur = *scrolling_target;
			}
		}
	}

	pub fn draw_scrollbars(&mut self, state: &mut DrawState, params: &DrawParams, info: &ScrollbarInfo) {
		let (enabled_horiz, enabled_vert) = get_scroll_enabled(params.style);
		if !enabled_horiz && !enabled_vert {
			return;
		}

		let transform = state.transform_stack.get();

		let thickness = 6.0;
		let margin = 4.0;

		let rect_params = drawing::Rectangle {
			color: drawing::Color::new(1.0, 1.0, 1.0, 0.0),
			border: 2.0,
			border_color: drawing::Color::new(1.0, 1.0, 1.0, 1.0),
			round_units: 2,
			..Default::default()
		};

		// Horizontal handle
		if enabled_horiz && info.handle_size.x < 1.0 {
			state.primitives.push(drawing::RenderPrimitive::Rectangle(
				PrimitiveExtent {
					boundary: drawing::Boundary::from_pos_size(
						Vec2::new(
							transform.abs_pos.x + transform.raw_dim.x * (1.0 - info.handle_size.x) * self.data.scrolling_cur.x,
							transform.abs_pos.y + transform.raw_dim.y - thickness - margin,
						),
						Vec2::new(transform.raw_dim.x * info.handle_size.x, thickness),
					),
					transform: transform.transform,
				},
				rect_params,
			));
		}

		// Vertical handle
		if enabled_vert && info.handle_size.y < 1.0 {
			state.primitives.push(drawing::RenderPrimitive::Rectangle(
				PrimitiveExtent {
					boundary: drawing::Boundary::from_pos_size(
						Vec2::new(
							transform.abs_pos.x + transform.raw_dim.x - thickness - margin,
							transform.abs_pos.y + transform.raw_dim.y * (1.0 - info.handle_size.y) * self.data.scrolling_cur.y,
						),
						Vec2::new(thickness, transform.raw_dim.y * info.handle_size.y),
					),
					transform: transform.transform,
				},
				rect_params,
			));
		}
	}

	fn process_wheel(&mut self, params: &mut EventParams, wheel: &MouseWheelEvent) -> bool {
		let (enabled_horiz, enabled_vert) = get_scroll_enabled(params.style);
		if !enabled_horiz && !enabled_vert {
			return false;
		}

		let l = params.layout;
		let overflow = Vec2::new(l.scroll_width(), l.scroll_height());
		if overflow.x == 0.0 && overflow.y == 0.0 {
			return false; // not overflowing
		}

		let Some(info) = get_scrollbar_info(params.layout) else {
			return false;
		};

		let step_pixels = 64.0;

		if info.handle_size.x < 1.0 && wheel.pos.x != 0.0 {
			// Horizontal scrolling
			let mult = (1.0 / (l.content_box_width() - info.content_size.x)) * step_pixels;
			let new_scroll = (self.data.scrolling_target.x + wheel.shift.x * mult).clamp(0.0, 1.0);
			if self.data.scrolling_target.x != new_scroll {
				self.data.scrolling_target.x = new_scroll;
				params.alterables.mark_tick(self.obj.get_id());
			}
		}

		if info.handle_size.y < 1.0 && wheel.pos.y != 0.0 {
			// Vertical scrolling
			let mult = (1.0 / (l.content_box_height() - info.content_size.y)) * step_pixels;
			let new_scroll = (self.data.scrolling_target.y + wheel.shift.y * mult).clamp(0.0, 1.0);
			if self.data.scrolling_target.y != new_scroll {
				self.data.scrolling_target.y = new_scroll;
				params.alterables.mark_tick(self.obj.get_id());
			}
		}

		true
	}

	#[allow(clippy::too_many_lines)]
	#[allow(clippy::cognitive_complexity)]
	pub fn process_event<'a, U1: 'static, U2: 'static>(
		&mut self,
		widget_id: WidgetID,
		node_id: taffy::NodeId,
		event: &Event,
		user_data: &mut (&mut U1, &mut U2),
		params: &'a mut EventParams<'a>,
	) -> anyhow::Result<EventResult> {
		let hovered = event.test_mouse_within_transform(params.alterables.transform_stack.get());

		match &event {
			Event::MouseDown(e) => {
				if hovered && self.data.set_device_pressed(e.device, true) {
					call_event!(
						self,
						widget_id,
						node_id,
						params,
						MousePress,
						U1,
						U2,
						user_data,
						CallbackMetadata::MouseButton(event::MouseButton {
							index: e.index,
							pos: e.pos
						})
					);
				}
			}
			Event::MouseUp(e) => {
				if self.data.set_device_pressed(e.device, false) {
					call_event!(
						self,
						widget_id,
						node_id,
						params,
						MouseRelease,
						U1,
						U2,
						user_data,
						CallbackMetadata::MouseButton(event::MouseButton {
							index: e.index,
							pos: e.pos,
						})
					);
				}
			}
			Event::MouseMotion(e) => {
				let hover_state_changed = self.data.set_device_hovered(e.device, hovered);

				if hover_state_changed {
					if self.data.is_hovered() {
						call_event!(
							self,
							widget_id,
							node_id,
							params,
							MouseEnter,
							U1,
							U2,
							user_data,
							CallbackMetadata::None
						);
					} else {
						call_event!(
							self,
							widget_id,
							node_id,
							params,
							MouseLeave,
							U1,
							U2,
							user_data,
							CallbackMetadata::None
						);
					}

					call_event!(
						self,
						widget_id,
						node_id,
						params,
						MouseMotion,
						U1,
						U2,
						user_data,
						CallbackMetadata::MousePosition(event::MousePosition { pos: e.pos })
					);
				}
			}
			Event::MouseWheel(e) => {
				if hovered && self.process_wheel(params, e) {
					return Ok(EventResult::Consumed);
				}
			}
			Event::MouseLeave(e) => {
				if self.data.set_device_hovered(e.device, false) {
					call_event!(
						self,
						widget_id,
						node_id,
						params,
						MouseLeave,
						U1,
						U2,
						user_data,
						CallbackMetadata::None
					);
				}
			}
			Event::InternalStateChange(e) => {
				call_event!(
					self,
					widget_id,
					node_id,
					params,
					InternalStateChange,
					U1,
					U2,
					user_data,
					CallbackMetadata::Custom(e.metadata)
				);
			}
		}
		Ok(EventResult::Pass)
	}
}

pub struct ConstructEssentials<'a> {
	pub layout: &'a mut Layout,
	pub parent: WidgetID,
}
