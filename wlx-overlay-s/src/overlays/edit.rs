use std::{
    any::Any,
    cell::RefCell,
    mem::{self, ManuallyDrop},
    rc::Rc,
    time::{Duration, Instant},
};

use glam::{vec2, FloatExt, UVec2};
use slotmap::Key;
use wgui::{
    animation::{Animation, AnimationEasing},
    components::{checkbox::ComponentCheckbox, slider::ComponentSlider},
    event::EventCallback,
    layout::{Layout, WidgetID},
    parser::{CustomAttribsInfoOwned, Fetchable},
    widget::{rectangle::WidgetRectangle, EventResult},
};

#[cfg(feature = "wayvr")]
use crate::{backend::task::TaskType, windowing::OverlaySelector};
use crate::{
    backend::{input::HoverResult, task::TaskContainer},
    gui::panel::{button::BUTTON_EVENTS, GuiPanel, NewGuiPanelParams},
    state::AppState,
    subsystem::hid::WheelDelta,
    windowing::{
        backend::{DummyBackend, OverlayBackend, RenderResources, ShouldRender},
        window::{OverlayWindowConfig, Positioning},
        OverlayID,
    },
};

struct LongPressButtonState {
    pressed: Instant,
}

struct EditModeState {
    tasks: Rc<RefCell<TaskContainer>>,
    id: Rc<RefCell<OverlayID>>,
    interact_lock: bool,
    positioning: Positioning,
    delete: LongPressButtonState,
    rect_id: WidgetID,
    rect_color: wgui::drawing::Color,
    border_color: wgui::drawing::Color,
}

type EditModeWrapPanel = GuiPanel<EditModeState>;

#[derive(Default)]
pub struct EditWrapperManager {
    edit_mode: bool,
    panel_pool: Vec<EditModeWrapPanel>,
}

impl EditWrapperManager {
    pub fn wrap_edit_mode(
        &mut self,
        id: OverlayID,
        owc: &mut OverlayWindowConfig,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        if owc.editing {
            return Ok(());
        }

        let Some(meta) = owc.backend.frame_meta() else {
            log::error!("META NULL");
            return Ok(());
        };

        log::debug!("EditMode wrap on {}", owc.name);
        let inner = mem::replace(&mut owc.backend, Box::new(DummyBackend {}));
        let mut panel = self.panel_pool.pop();
        if panel.is_none() {
            panel = Some(make_edit_panel(
                app,
                UVec2::new(meta.extent[0], meta.extent[1]),
            )?);
        }
        let mut panel = panel.unwrap();
        panel_new_assignment(&mut panel, id, owc, app)?;

        owc.backend = Box::new(EditModeBackendWrapper {
            inner: ManuallyDrop::new(inner),
            panel: ManuallyDrop::new(panel),
        });
        owc.editing = true;

        Ok(())
    }

    pub fn unwrap_edit_mode(&mut self, owc: &mut OverlayWindowConfig) {
        if !owc.editing {
            return;
        }

        log::debug!("EditMode unwrap on {}", owc.name);
        let wrapper = mem::replace(&mut owc.backend, Box::new(DummyBackend {}));
        let mut wrapper: Box<dyn Any> = wrapper;
        let wrapper = wrapper
            .downcast_mut::<EditModeBackendWrapper>()
            .expect("Wrong type to unwrap");

        let panel = unsafe { ManuallyDrop::take(&mut wrapper.panel) };
        self.panel_pool.push(panel);

        let inner = unsafe { ManuallyDrop::take(&mut wrapper.inner) };
        owc.backend = inner;
        owc.editing = false;

        // wrapper is destroyed with nothing left inside
    }
}

pub struct EditModeBackendWrapper {
    panel: ManuallyDrop<EditModeWrapPanel>,
    inner: ManuallyDrop<Box<dyn OverlayBackend>>,
}

impl OverlayBackend for EditModeBackendWrapper {
    fn init(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.init(app)?;
        self.panel.init(app)
    }
    fn pause(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.pause(app)?;
        self.panel.pause(app)
    }
    fn resume(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.resume(app)?;
        self.panel.resume(app)
    }
    fn should_render(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<ShouldRender> {
        {
            let mut local_tasks = self.panel.state.tasks.borrow_mut();
            app.tasks.transfer_from(&mut local_tasks);
        }

        let i = self.inner.should_render(app)?;

        if !matches!(i, ShouldRender::Unable)
            && let Some(ref frame_meta) = self.inner.frame_meta()
        {
            let (width_px, height_px) = (frame_meta.extent[0], frame_meta.extent[1]);

            let new_size = vec2(width_px as _, height_px as _);
            if self.panel.max_size != new_size {
                log::debug!("EditWrapperGui size {} → {new_size}", self.panel.max_size);
                self.panel.max_size = new_size;

                let gui_scale = width_px.min(height_px) as f32 / 550.0;
                self.panel.gui_scale = gui_scale;
                self.panel.update_layout()?;
            }
        } else {
            return Ok(ShouldRender::Unable);
        }

        let p = self.panel.should_render(app)?;

        #[allow(clippy::match_same_arms)]
        Ok(match (i, p) {
            (ShouldRender::Should, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Should, ShouldRender::Can) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Can) => ShouldRender::Can,
            _ => ShouldRender::Unable,
        })
    }
    fn render(
        &mut self,
        app: &mut crate::state::AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        self.inner.render(app, rdr)?;
        self.panel.render(app, rdr)
    }
    fn frame_meta(&mut self) -> Option<crate::windowing::backend::FrameMeta> {
        self.inner.frame_meta()
    }
    fn on_hover(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
    ) -> HoverResult {
        // pass through hover events to force pipewire to capture frames for us
        let _ = self.inner.on_hover(app, hit);
        self.panel.on_hover(app, hit)
    }
    fn on_left(&mut self, app: &mut crate::state::AppState, pointer: usize) {
        self.inner.on_left(app, pointer);
        self.panel.on_left(app, pointer);
    }
    fn on_pointer(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
        pressed: bool,
    ) {
        self.panel.on_pointer(app, hit, pressed);
    }
    fn on_scroll(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
        delta: WheelDelta,
    ) {
        self.panel.on_scroll(app, hit, delta);
    }
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        self.inner.get_interaction_transform()
    }
}

fn make_edit_panel(
    app: &mut AppState,
    overlay_resolution: UVec2,
) -> anyhow::Result<EditModeWrapPanel> {
    log::error!(
        "overlay res {} {}",
        overlay_resolution.x,
        overlay_resolution.y
    );

    let state = EditModeState {
        id: Rc::new(RefCell::new(OverlayID::null())),
        interact_lock: false,
        positioning: Positioning::Static,
        tasks: Rc::new(RefCell::new(TaskContainer::new())),
        delete: LongPressButtonState {
            pressed: Instant::now(),
        },
        rect_id: WidgetID::null(),
        rect_color: wgui::drawing::Color::default(),
        border_color: wgui::drawing::Color::default(),
    };

    let on_custom_attrib: Box<dyn Fn(&mut Layout, &CustomAttribsInfoOwned, &AppState)> =
        Box::new(move |layout, attribs, _app| {
            for (name, kind) in &BUTTON_EVENTS {
                let Some(action) = attribs.get_value(name) else {
                    continue;
                };

                let mut args = action.split_whitespace();
                let Some(command) = args.next() else {
                    continue;
                };

                let callback: EventCallback<AppState, EditModeState> = match command {
                    "::EditModeToggleLock" => Box::new(move |common, _data, app, state| {
                        state.interact_lock = !state.interact_lock;

                        let defaults = app.wgui_globals.get().defaults.clone();
                        let rect_color = state.rect_color.clone();
                        let border_color = state.border_color.clone();

                        if state.interact_lock {
                            common.alterables.animate(Animation::new(
                                state.rect_id,
                                10,
                                AnimationEasing::OutBack,
                                Box::new(move |common, data| {
                                    let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
                                    set_anim_color(
                                        rect,
                                        data.pos * 0.2,
                                        rect_color,
                                        border_color,
                                        defaults.danger_color,
                                    );
                                    common.alterables.mark_redraw();
                                }),
                            ));
                        } else {
                            common.alterables.animate(Animation::new(
                                state.rect_id,
                                10,
                                AnimationEasing::OutQuad,
                                Box::new(move |common, data| {
                                    let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
                                    set_anim_color(
                                        rect,
                                        0.2 - (data.pos * 0.2),
                                        rect_color,
                                        border_color,
                                        defaults.danger_color,
                                    );
                                    common.alterables.mark_redraw();
                                }),
                            ));
                        };

                        let interactable = !state.interact_lock;
                        app.tasks.enqueue(TaskType::Overlay(
                            OverlaySelector::Id(state.id.borrow().clone()),
                            Box::new(move |_app, owc| {
                                let state = owc.active_state.as_mut().unwrap(); //want panic
                                state.interactable = interactable;
                            }),
                        ));
                        Ok(EventResult::Consumed)
                    }),
                    "::EditModeDeletePress" => Box::new(move |_common, _data, _app, state| {
                        state.delete.pressed = Instant::now();
                        // TODO: animate to light up button after 2s
                        Ok(EventResult::Consumed)
                    }),
                    "::EditModeDeleteRelease" => Box::new(move |_common, _data, app, state| {
                        if state.delete.pressed.elapsed() > Duration::from_secs(2) {
                            return Ok(EventResult::Pass);
                        }
                        app.tasks.enqueue(TaskType::DropOverlay(OverlaySelector::Id(
                            state.id.borrow().clone(),
                        )));
                        Ok(EventResult::Consumed)
                    }),
                    _ => return,
                };

                let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
                log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
            }
        });

    let mut panel = GuiPanel::new_from_template(
        app,
        "gui/edit.xml",
        state,
        NewGuiPanelParams {
            on_custom_attrib: Some(on_custom_attrib),
            resize_to_parent: true,
            ..Default::default()
        },
    )?;

    set_up_shadow(&mut panel)?;
    set_up_checkbox(&mut panel, "additive_box", cb_assign_additive)?;
    set_up_slider(&mut panel, "alpha_slider", cb_assign_alpha)?;
    set_up_slider(&mut panel, "curve_slider", cb_assign_curve)?;

    Ok(panel)
}

fn cb_assign_alpha(_app: &mut AppState, owc: &mut OverlayWindowConfig, alpha: f32) {
    owc.dirty = true;
    owc.active_state.as_mut().unwrap().alpha = alpha;
}

fn cb_assign_curve(_app: &mut AppState, owc: &mut OverlayWindowConfig, curvature: f32) {
    owc.dirty = true;
    owc.active_state.as_mut().unwrap().curvature = if curvature < 0.005 {
        None
    } else {
        Some(curvature)
    };
}

fn cb_assign_additive(_app: &mut AppState, owc: &mut OverlayWindowConfig, additive: bool) {
    owc.dirty = true;
    owc.active_state.as_mut().unwrap().additive = additive;
}

fn set_up_slider(
    panel: &mut EditModeWrapPanel,
    id: &str,
    callback: fn(&mut AppState, &mut OverlayWindowConfig, f32),
) -> anyhow::Result<()> {
    let slider = panel
        .parser_state
        .fetch_component_as::<ComponentSlider>(id)?;
    let tasks = panel.state.tasks.clone();
    let overlay_id = panel.state.id.clone();
    slider.on_value_changed(Box::new(move |_common, e| {
        let mut tasks = tasks.borrow_mut();
        let e_value = e.value;

        tasks.enqueue(TaskType::Overlay(
            OverlaySelector::Id(overlay_id.borrow().clone()),
            Box::new(move |app, owc| callback(app, owc, e_value)),
        ));
        Ok(())
    }));

    Ok(())
}

fn set_up_checkbox(
    panel: &mut EditModeWrapPanel,
    id: &str,
    callback: fn(&mut AppState, &mut OverlayWindowConfig, bool),
) -> anyhow::Result<()> {
    let checkbox = panel
        .parser_state
        .fetch_component_as::<ComponentCheckbox>(id)?;
    let tasks = panel.state.tasks.clone();
    let overlay_id = panel.state.id.clone();
    checkbox.on_toggle(Box::new(move |_common, e| {
        let mut tasks = tasks.borrow_mut();
        let e_checked = e.checked;

        tasks.enqueue(TaskType::Overlay(
            OverlaySelector::Id(overlay_id.borrow().clone()),
            Box::new(move |app, owc| callback(app, owc, e_checked)),
        ));
        Ok(())
    }));

    Ok(())
}

fn set_up_shadow(panel: &mut EditModeWrapPanel) -> anyhow::Result<()> {
    panel.state.rect_id = panel.parser_state.get_widget_id("shadow")?;
    let shadow_rect = panel
        .layout
        .state
        .widgets
        .get_as::<WidgetRectangle>(panel.state.rect_id)
        .ok_or_else(|| anyhow::anyhow!("Element with id=\"shadow\" must be a <rectangle>"))?;
    panel.state.rect_color = shadow_rect.params.color;
    panel.state.border_color = shadow_rect.params.border_color;
    Ok(())
}

fn panel_new_assignment(
    panel: &mut EditModeWrapPanel,
    id: OverlayID,
    owc: &mut OverlayWindowConfig,
    app: &mut AppState,
) -> anyhow::Result<()> {
    *panel.state.id.borrow_mut() = id;
    let active_state = owc.active_state.as_mut().unwrap();
    panel.state.interact_lock = !active_state.interactable;
    panel.state.positioning = active_state.positioning;

    let alpha = active_state.alpha;
    let c = panel
        .parser_state
        .fetch_component_as::<ComponentSlider>("alpha_slider")?;
    panel.component_make_call(c, Box::new(move |c, cdc| c.set_value(cdc, alpha)))?;

    let curve = active_state.curvature.unwrap_or(0.0);
    let c = panel
        .parser_state
        .fetch_component_as::<ComponentSlider>("curve_slider")?;
    panel.component_make_call(c, Box::new(move |c, cdc| c.set_value(cdc, curve)))?;

    let additive = active_state.additive;
    let c = panel
        .parser_state
        .fetch_component_as::<ComponentCheckbox>("additive_box")?;
    panel.component_make_call(c, Box::new(move |c, cdc| c.set_checked(cdc, additive)))?;

    let mut rect = panel
        .layout
        .state
        .widgets
        .get_as::<WidgetRectangle>(panel.state.rect_id)
        .unwrap(); // can only fail if set_up_rect has issues

    if active_state.interactable {
        set_anim_color(
            &mut rect,
            0.0,
            panel.state.rect_color,
            panel.state.border_color,
            app.wgui_globals.get().defaults.danger_color,
        );
    } else {
        set_anim_color(
            &mut rect,
            0.2,
            panel.state.rect_color,
            panel.state.border_color,
            app.wgui_globals.get().defaults.danger_color,
        );
    }

    Ok(())
}

fn set_anim_color(
    rect: &mut WidgetRectangle,
    pos: f32,
    rect_color: wgui::drawing::Color,
    border_color: wgui::drawing::Color,
    target_color: wgui::drawing::Color,
) {
    // rect to target_color
    rect.params.color.r = rect_color.r.lerp(target_color.r, pos);
    rect.params.color.g = rect_color.g.lerp(target_color.g, pos);
    rect.params.color.b = rect_color.b.lerp(target_color.b, pos);

    // border to white
    rect.params.border_color.r = border_color.r.lerp(1.0, pos);
    rect.params.border_color.g = border_color.g.lerp(1.0, pos);
    rect.params.border_color.b = border_color.b.lerp(1.0, pos);
    rect.params.border_color.a = border_color.a.lerp(1.0, pos);
}
