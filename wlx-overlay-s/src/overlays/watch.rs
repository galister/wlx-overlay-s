use std::{
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use glam::{Affine3A, Vec3, Vec3A};
use wgui::{
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables, EventCallback},
    i18n::Translation,
    layout::WidgetID,
    parser::Fetchable,
    taffy,
    widget::EventResult,
};
use wlx_common::windowing::{OverlayWindowState, Positioning};

use crate::{
    backend::task::{ManagerTask, TaskType},
    gui::{
        panel::{button::BUTTON_EVENTS, GuiPanel, NewGuiPanelParams, OnCustomAttribFunc},
        timer::GuiTimer,
    },
    overlays::edit::LongPressButtonState,
    state::AppState,
    windowing::{
        backend::{OverlayEventData, OverlayMeta},
        manager::MAX_OVERLAY_SETS,
        window::{OverlayWindowConfig, OverlayWindowData},
        OverlaySelector, Z_ORDER_WATCH,
    },
};

pub const WATCH_NAME: &str = "watch";
const MAX_TOOLBOX_BUTTONS: usize = 16;

#[derive(Default)]
struct WatchState {
    current_set: Option<usize>,
    set_buttons: Vec<Rc<ComponentButton>>,
    overlay_buttons: Vec<Rc<ComponentButton>>,
    overlay_metas: Vec<OverlayMeta>,
    edit_mode_widgets: Vec<(WidgetID, bool)>,
    edit_add_widget: WidgetID,
    num_sets: usize,
    delete: LongPressButtonState,
}

#[allow(clippy::significant_drop_tightening)]
pub fn create_watch(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let state = WatchState::default();

    let on_custom_attrib: OnCustomAttribFunc = Box::new(move |layout, attribs, _app| {
        for (name, kind) in &BUTTON_EVENTS {
            let Some(action) = attribs.get_value(name) else {
                continue;
            };

            let mut args = action.split_whitespace();
            let Some(command) = args.next() else {
                continue;
            };

            let callback: EventCallback<AppState, WatchState> = match command {
                "::EditModeDeleteDown" => Box::new(move |_common, _data, _app, state| {
                    state.delete.pressed = Instant::now();
                    Ok(EventResult::Consumed)
                }),
                "::EditModeDeleteUp" => Box::new(move |_common, _data, app, state| {
                    if state.delete.pressed.elapsed() < Duration::from_secs(1) {
                        return Ok(EventResult::Consumed);
                    }
                    app.tasks
                        .enqueue(TaskType::Manager(ManagerTask::DeleteActiveSet));
                    Ok(EventResult::Consumed)
                }),
                "::EditModeAddSet" => Box::new(move |_common, _data, app, _state| {
                    app.tasks.enqueue(TaskType::Manager(ManagerTask::AddSet));
                    Ok(EventResult::Consumed)
                }),
                "::EditModeOverlayToggle" => {
                    let arg = args.next().unwrap_or_default();
                    let Ok(idx) = arg.parse::<usize>() else {
                        log::error!("{command} has invalid argument: \"{arg}\"");
                        return;
                    };
                    Box::new(move |_common, _data, app, state| {
                        let Some(overlay) = state.overlay_metas.get(idx) else {
                            log::error!("No overlay at index {idx}.");
                            return Ok(EventResult::Consumed);
                        };

                        app.tasks.enqueue(TaskType::Overlay(
                            OverlaySelector::Id(overlay.id.clone()),
                            Box::new(move |app, owc| {
                                if owc.active_state.is_none() {
                                    owc.activate(app);
                                } else {
                                    owc.deactivate();
                                }
                            }),
                        ));
                        Ok(EventResult::Consumed)
                    })
                }
                _ => return,
            };

            let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
            log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
        }
    });

    let mut panel = GuiPanel::new_from_template(
        app,
        "gui/watch.xml",
        state,
        NewGuiPanelParams {
            on_custom_id: Some(Box::new(
                move |id, widget, doc_params, layout, parser_state, state| {
                    if id.starts_with("norm_") {
                        state.edit_mode_widgets.push((widget, false));
                    } else if &*id == "edit_add" {
                        state.edit_add_widget = widget;
                    } else if id.starts_with("edit_") {
                        state.edit_mode_widgets.push((widget, true));
                    } else if &*id == "sets" {
                        for idx in 0..MAX_OVERLAY_SETS {
                            let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                            params.insert("display".into(), (idx + 1).to_string().into());
                            params.insert("handle".into(), idx.to_string().into());
                            parser_state
                                .instantiate_template(doc_params, "Set", layout, widget, params)?;

                            let button_id = format!("set_{idx}");
                            let component =
                                parser_state.fetch_component_as::<ComponentButton>(&button_id)?;
                            state.set_buttons.push(component);
                        }
                    } else if &*id == "toolbox" {
                        for idx in 0..MAX_TOOLBOX_BUTTONS {
                            let overlay_id = format!("overlay_{idx}");
                            let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                            params.insert("idx".into(), idx.to_string().into());
                            parser_state.instantiate_template(
                                doc_params, "Overlay", layout, widget, params,
                            )?;

                            let component =
                                parser_state.fetch_component_as::<ComponentButton>(&overlay_id)?;
                            state.overlay_buttons.push(component);
                        }
                    }
                    Ok(())
                },
            )),
            on_custom_attrib: Some(on_custom_attrib),
            ..Default::default()
        },
    )?;

    panel.on_notify = Some(Box::new(|panel, _app, event_data| {
        let mut alterables = EventAlterables::default();
        let mut common = CallbackDataCommon {
            alterables: &mut alterables,
            state: &panel.layout.state,
        };

        match event_data {
            OverlayEventData::ActiveSetChanged(current_set) => {
                if let Some(old_set) = panel.state.current_set.take() {
                    panel.state.set_buttons[old_set].set_sticky_state(&mut common, false);
                }
                if let Some(new_set) = current_set {
                    panel.state.set_buttons[new_set].set_sticky_state(&mut common, true);
                }
                panel.state.current_set = current_set;
            }
            OverlayEventData::NumSetsChanged(num_sets) => {
                panel.state.num_sets = num_sets;
                for i in 0..MAX_OVERLAY_SETS {
                    let comp = panel.state.set_buttons[i].clone();
                    let rect_id = comp.get_rect();
                    let display = if i < num_sets {
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    panel.widget_set_display(rect_id, display, &mut common.alterables);
                }
                let display = if num_sets < 7 {
                    taffy::Display::Flex
                } else {
                    taffy::Display::None
                };
                panel.widget_set_display(
                    panel.state.edit_add_widget,
                    display,
                    &mut common.alterables,
                );
            }
            OverlayEventData::EditModeChanged(edit_mode) => {
                for (w, e) in panel.state.edit_mode_widgets.iter() {
                    let display = if *e == edit_mode {
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    panel.widget_set_display(*w, display, &mut common.alterables);
                }
                let display = if edit_mode && panel.state.num_sets < 7 {
                    taffy::Display::Flex
                } else {
                    taffy::Display::None
                };
                panel.widget_set_display(
                    panel.state.edit_add_widget,
                    display,
                    &mut common.alterables,
                );
            }
            OverlayEventData::OverlaysChanged(metas) => {
                panel.state.overlay_metas = metas;
                for (idx, btn) in panel.state.overlay_buttons.iter().enumerate() {
                    let display = if let Some(meta) = panel.state.overlay_metas.get(idx) {
                        btn.set_text(&mut common, Translation::from_raw_text(&meta.name));
                        //TODO: add category icons
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    panel.widget_set_display(btn.get_rect(), display, &mut common.alterables);
                }
            }
        }

        panel.layout.process_alterables(alterables)?;
        Ok(())
    }));

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: app.session.config.watch_hand,
        lerp: 1.0,
    };

    panel.update_layout()?;

    Ok(OverlayWindowConfig {
        name: WATCH_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                app.session.config.watch_rot,
                app.session.config.watch_pos,
            ),
            ..OverlayWindowState::default()
        },
        show_on_spawn: true,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

pub fn watch_fade<D>(app: &mut AppState, watch: &mut OverlayWindowData<D>) {
    let Some(state) = watch.config.active_state.as_mut() else {
        return;
    };

    let to_hmd = (state.transform.translation - app.input_state.hmd.translation).normalize();
    let watch_normal = state.transform.transform_vector3a(Vec3A::NEG_Z).normalize();
    let dot = to_hmd.dot(watch_normal);

    state.alpha = (dot - app.session.config.watch_view_angle_min)
        / (app.session.config.watch_view_angle_max - app.session.config.watch_view_angle_min);
    state.alpha += 0.1;
    state.alpha = state.alpha.clamp(0., 1.);
}
