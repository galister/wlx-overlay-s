use glam::Vec2;

use super::drawing::RenderPrimitive;
use crate::{
	animation,
	any::AnyTrait,
	drawing,
	event::{CallbackData, Event, EventListener, MouseWheelEvent},
	layout::{Layout, WidgetID, WidgetMap},
	transform_stack::TransformStack,
};

pub mod div;
pub mod rectangle;
pub mod sprite;
pub mod text;
pub mod util;

pub struct WidgetData {
	hovered: usize,
	pressed: usize,
	pub scrolling: Vec2, // normalized, 0.0-1.0. Not used in case if overflow != scroll
	pub transform: glam::Mat4,
}

impl WidgetData {
	pub fn set_device_pressed(&mut self, device: usize, pressed: bool) -> bool {
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

	pub fn set_device_hovered(&mut self, device: usize, hovered: bool) -> bool {
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

	pub fn get_pressed(&self, device: usize) -> bool {
		self.pressed & (1 << device) != 0
	}

	pub fn get_hovered(&self, device: usize) -> bool {
		self.hovered & (1 << device) != 0
	}

	pub fn is_pressed(&self) -> bool {
		self.pressed != 0
	}

	pub fn is_hovered(&self) -> bool {
		self.hovered != 0
	}
}

pub struct WidgetState {
	pub data: WidgetData,
	pub obj: Box<dyn WidgetObj>,
	pub event_listeners: Vec<EventListener>,
}

impl WidgetState {
	fn new(obj: Box<dyn WidgetObj>) -> anyhow::Result<WidgetState> {
		Ok(Self {
			data: WidgetData {
				hovered: 0,
				pressed: 0,
				scrolling: Vec2::default(),
				transform: glam::Mat4::IDENTITY,
			},
			event_listeners: Vec::new(),
			obj,
		})
	}
}

// global draw params
pub struct DrawState<'a> {
	pub layout: &'a Layout,
	pub primitives: &'a mut Vec<RenderPrimitive>,
	pub transform_stack: &'a mut TransformStack,
	pub depth: f32, //TODO: actually use this in shader
}

// per-widget draw params
pub struct DrawParams<'a> {
	pub node_id: taffy::NodeId,
	pub style: &'a taffy::Style,
	pub taffy_layout: &'a taffy::Layout,
}

pub trait WidgetObj: AnyTrait {
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
	pub taffy_layout: &'a taffy::Layout,
	pub widgets: &'a WidgetMap,
	pub tree: &'a taffy::TaffyTree<WidgetID>,
	pub transform_stack: &'a TransformStack,
	pub animations: &'a mut Vec<animation::Animation>,
	pub needs_redraw: &'a mut bool,
	pub dirty_nodes: &'a mut Vec<taffy::NodeId>,
}

pub enum EventResult {
	Pass,
	Consumed,
	Outside,
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
	pub fn get_as<T: 'static>(&self) -> &T {
		let any = self.as_any();
		any.downcast_ref::<T>().unwrap()
	}

	pub fn get_as_mut<T: 'static>(&mut self) -> &mut T {
		let any = self.as_any_mut();
		any.downcast_mut::<T>().unwrap()
	}
}

impl WidgetState {
	pub fn add_event_listener(&mut self, listener: EventListener) {
		self.event_listeners.push(listener);
	}

	pub fn get_scroll_shift(&self, info: &ScrollbarInfo, l: &taffy::Layout) -> Vec2 {
		Vec2::new(
			(info.content_size.x - l.content_box_width()) * self.data.scrolling.x,
			(info.content_size.y - l.content_box_height()) * self.data.scrolling.y,
		)
	}

	pub fn draw_all(&mut self, state: &mut DrawState, params: &DrawParams) {
		self.obj.draw(state, params);
	}

	pub fn draw_scrollbars(
		&mut self,
		state: &mut DrawState,
		params: &DrawParams,
		info: &ScrollbarInfo,
	) {
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
			state.primitives.push(drawing::RenderPrimitive {
				boundary: drawing::Boundary::from_pos_size(
					Vec2::new(
						transform.pos.x + transform.dim.x * (1.0 - info.handle_size.x) * self.data.scrolling.x,
						transform.pos.y + transform.dim.y - thickness - margin,
					),
					Vec2::new(transform.dim.x * info.handle_size.x, thickness),
				),
				depth: state.depth,
				transform: transform.transform,
				payload: drawing::PrimitivePayload::Rectangle(rect_params),
			});
		}

		// Vertical handle
		if enabled_vert && info.handle_size.y < 1.0 {
			state.primitives.push(drawing::RenderPrimitive {
				boundary: drawing::Boundary::from_pos_size(
					Vec2::new(
						transform.pos.x + transform.dim.x - thickness - margin,
						transform.pos.y + transform.dim.y * (1.0 - info.handle_size.y) * self.data.scrolling.y,
					),
					Vec2::new(thickness, transform.dim.y * info.handle_size.y),
				),
				depth: state.depth,
				transform: transform.transform,
				payload: drawing::PrimitivePayload::Rectangle(rect_params),
			});
		}
	}

	fn process_wheel(&mut self, params: &mut EventParams, wheel: &MouseWheelEvent) -> bool {
		let (enabled_horiz, enabled_vert) = get_scroll_enabled(params.style);
		if !enabled_horiz && !enabled_vert {
			return false;
		}

		let l = params.taffy_layout;
		let overflow = Vec2::new(l.scroll_width(), l.scroll_height());
		if overflow.x == 0.0 && overflow.y == 0.0 {
			return false; // not overflowing
		}

		let Some(info) = get_scrollbar_info(params.taffy_layout) else {
			return false;
		};

		let step_pixels = 32.0;

		if info.handle_size.x < 1.0 && wheel.pos.x != 0.0 {
			// Horizontal scrolling
			let mult = (1.0 / (l.content_box_width() - info.content_size.x)) * step_pixels;
			let new_scroll = (self.data.scrolling.x + wheel.shift.x * mult).clamp(0.0, 1.0);
			if self.data.scrolling.x != new_scroll {
				self.data.scrolling.x = new_scroll;
				*params.needs_redraw = true;
			}
		}

		if info.handle_size.y < 1.0 && wheel.pos.y != 0.0 {
			// Vertical scrolling
			let mult = (1.0 / (l.content_box_height() - info.content_size.y)) * step_pixels;
			let new_scroll = (self.data.scrolling.y + wheel.shift.y * mult).clamp(0.0, 1.0);
			if self.data.scrolling.y != new_scroll {
				self.data.scrolling.y = new_scroll;
				*params.needs_redraw = true;
			}
		}

		true
	}

	pub fn process_event(
		&mut self,
		widget_id: WidgetID,
		node_id: taffy::NodeId,
		event: &Event,
		params: &mut EventParams,
	) -> EventResult {
		let hovered = event.test_mouse_within_transform(params.transform_stack.get());

		let mut pressed_changed_button = None;
		let mut hovered_changed = false;

		match &event {
			Event::MouseDown(e) => {
				pressed_changed_button = self
					.data
					.set_device_pressed(e.device, true)
					.then_some(e.button);
			}
			Event::MouseUp(e) => {
				pressed_changed_button = self
					.data
					.set_device_pressed(e.device, false)
					.then_some(e.button);
			}
			Event::MouseWheel(e) => {
				if self.process_wheel(params, e) {
					return EventResult::Consumed;
				}
			}
			Event::MouseMotion(e) => {
				hovered_changed |= self.data.set_device_hovered(e.device, hovered);
			}
			Event::MouseLeave(e) => {
				hovered_changed |= self.data.set_device_hovered(e.device, false);
			}
			_ => {}
		}

		for listener in &self.event_listeners {
			match listener {
				EventListener::MouseEnter(callback) => {
					if hovered_changed && self.data.is_hovered() {
						let mut data = CallbackData {
							obj: self.obj.as_mut(),
							widget_data: &mut self.data,
							widgets: params.widgets,
							animations: params.animations,
							dirty_nodes: params.dirty_nodes,
							widget_id,
							node_id,
							needs_redraw: false,
						};
						callback(&mut data);
						if data.needs_redraw {
							*params.needs_redraw = true;
						}
					}
				}
				EventListener::MouseLeave(callback) => {
					if hovered_changed && !self.data.is_hovered() {
						let mut data = CallbackData {
							obj: self.obj.as_mut(),
							widget_data: &mut self.data,
							widgets: params.widgets,
							animations: params.animations,
							dirty_nodes: params.dirty_nodes,
							widget_id,
							node_id,
							needs_redraw: false,
						};
						callback(&mut data);
						if data.needs_redraw {
							*params.needs_redraw = true;
						}
					}
				}
				EventListener::MousePress(callback) => {
					if let Some(button) = pressed_changed_button.filter(|_| self.data.is_pressed()) {
						let mut data = CallbackData {
							obj: self.obj.as_mut(),
							widget_data: &mut self.data,
							widgets: params.widgets,
							animations: params.animations,
							dirty_nodes: params.dirty_nodes,
							widget_id,
							node_id,
							needs_redraw: false,
						};
						callback(&mut data, button);
						if data.needs_redraw {
							*params.needs_redraw = true;
						}
					}
				}
				EventListener::MouseRelease(callback) => {
					if let Some(button) = pressed_changed_button.filter(|_| !self.data.is_pressed()) {
						let mut data = CallbackData {
							obj: self.obj.as_mut(),
							widget_data: &mut self.data,
							widgets: params.widgets,
							animations: params.animations,
							dirty_nodes: params.dirty_nodes,
							widget_id,
							node_id,
							needs_redraw: false,
						};
						callback(&mut data, button);
						if data.needs_redraw {
							*params.needs_redraw = true;
						}
					}
				}
				EventListener::InternalStateChange(callback) => {
					let mut data = CallbackData {
						obj: self.obj.as_mut(),
						widget_data: &mut self.data,
						widgets: params.widgets,
						animations: params.animations,
						dirty_nodes: params.dirty_nodes,
						widget_id,
						node_id,
						needs_redraw: false,
					};
					callback(&mut data);
					if data.needs_redraw {
						*params.needs_redraw = true;
					}
				}
			}
		}

		EventResult::Pass
	}
}
