use std::{
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use glam::{Affine3A, Vec3, Vec3A};
use idmap::DirectIdMap;
use wgui::{
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables, EventCallback, StyleSetRequest},
    i18n::Translation,
    layout::WidgetID,
    parser::Fetchable,
    renderer_vk::text::custom_glyph::CustomGlyphData,
    taffy,
    widget::{sprite::WidgetSprite, EventResult},
};
use wlx_common::windowing::{OverlayWindowState, Positioning};

use crate::{
    backend::{
        input::TrackedDeviceRole,
        task::{ManagerTask, TaskType},
    },
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
const MAX_DEVICES: usize = 9;

#[derive(Default)]
struct WatchState {
    current_set: Option<usize>,
    set_buttons: Vec<Rc<ComponentButton>>,
    overlay_buttons: Vec<Rc<ComponentButton>>,
    overlay_metas: Vec<OverlayMeta>,
    edit_mode_widgets: Vec<(WidgetID, bool)>,
    edit_add_widget: WidgetID,
    device_role_icons: DirectIdMap<TrackedDeviceRole, CustomGlyphData>,
    devices: Vec<(WidgetID, WidgetID)>,
    num_sets: usize,
    delete: LongPressButtonState,
}

#[allow(clippy::significant_drop_tightening)]
#[allow(clippy::too_many_lines)]
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
                            OverlaySelector::Id(overlay.id),
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
                        let node = layout.state.nodes[widget];
                        let num_children = layout.state.tree.children(node).iter().len();

                        for idx in 0..MAX_OVERLAY_SETS {
                            if idx >= num_children {
                                let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                                params.insert("display".into(), (idx + 1).to_string().into());
                                params.insert("idx".into(), idx.to_string().into());
                                parser_state.instantiate_template(
                                    doc_params, "Set", layout, widget, params,
                                )?;
                            }

                            let comp = parser_state
                                .fetch_component_as::<ComponentButton>(&format!("set_{idx}"))?;
                            state.set_buttons.push(comp);
                        }
                    } else if &*id == "toolbox" {
                        let node = layout.state.nodes[widget];
                        let num_children = layout.state.tree.children(node).iter().len() - 1; // -1 for keyboard

                        for idx in 0..MAX_TOOLBOX_BUTTONS {
                            if idx >= num_children {
                                let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                                params.insert("idx".into(), idx.to_string().into());
                                parser_state.instantiate_template(
                                    doc_params, "Overlay", layout, widget, params,
                                )?;
                            }

                            let comp = parser_state
                                .fetch_component_as::<ComponentButton>(&format!("overlay_{idx}"))?;
                            state.overlay_buttons.push(comp);
                        }
                    } else if id.starts_with("dev_") && id.ends_with("_sprite") {
                        // store device icons from xml
                        let id_n = id
                            .replace("dev_", "")
                            .replace("_sprite", "")
                            .parse::<u64>()?;

                        let role = match id_n {
                            0 => TrackedDeviceRole::Hmd,
                            1 => TrackedDeviceRole::LeftHand,
                            2 => TrackedDeviceRole::RightHand,
                            3 => TrackedDeviceRole::Tracker,
                            _ => return Ok(()), // not parsing the first 4 elems
                        };

                        let sprite = layout
                            .state
                            .widgets
                            .get_as::<WidgetSprite>(widget)
                            .ok_or_else(|| {
                                anyhow::anyhow!("{id} is expected to be a sprite, but it isn't.")
                            })?;

                        let src = sprite.params.glyph_data.clone().ok_or_else(|| {
                            anyhow::anyhow!("{id} is expected to have a src, but it doesn't.")
                        })?;

                        state.device_role_icons.insert(role, src);
                    } else if &*id == "devices" {
                        let node = layout.state.nodes[widget];
                        let num_children = layout.state.tree.children(node).iter().len();

                        for idx in 0..MAX_DEVICES {
                            if idx >= num_children {
                                let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                                params.insert("idx".into(), idx.to_string().into());
                                params.insert("src".into(), "".to_string().into());
                                parser_state.instantiate_template(
                                    doc_params, "Device", layout, widget, params,
                                )?;
                            }

                            let div = parser_state.get_widget_id(&format!("dev_{idx}"))?;
                            let spr = parser_state.get_widget_id(&format!("dev_{idx}_sprite"))?;
                            state.devices.push((div, spr));
                        }
                    }
                    Ok(())
                },
            )),
            on_custom_attrib: Some(on_custom_attrib),
            ..Default::default()
        },
    )?;

    panel.on_notify = Some(Box::new(|panel, app, event_data| {
        let mut alterables = EventAlterables::default();
        let mut com = CallbackDataCommon {
            alterables: &mut alterables,
            state: &panel.layout.state,
        };

        match event_data {
            OverlayEventData::ActiveSetChanged(current_set) => {
                if let Some(old_set) = panel.state.current_set.take() {
                    panel.state.set_buttons[old_set].set_sticky_state(&mut com, false);
                }
                if let Some(new_set) = current_set {
                    panel.state.set_buttons[new_set].set_sticky_state(&mut com, true);
                }
                panel.state.current_set = current_set;
            }
            OverlayEventData::NumSetsChanged(num_sets) => {
                panel.state.num_sets = num_sets;
                for (i, comp) in panel.state.set_buttons.iter().enumerate() {
                    let rect_id = comp.get_rect();
                    let display = if i < num_sets {
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    com.alterables
                        .set_style(rect_id, StyleSetRequest::Display(display));
                }
                let display = if num_sets < 7 {
                    taffy::Display::Flex
                } else {
                    taffy::Display::None
                };
                com.alterables.set_style(
                    panel.state.edit_add_widget,
                    StyleSetRequest::Display(display),
                );
            }
            OverlayEventData::EditModeChanged(edit_mode) => {
                for (w, e) in &panel.state.edit_mode_widgets {
                    let display = if *e == edit_mode {
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    com.alterables
                        .set_style(*w, StyleSetRequest::Display(display));
                }
                let display = if edit_mode && panel.state.num_sets < 7 {
                    taffy::Display::Flex
                } else {
                    taffy::Display::None
                };
                com.alterables.set_style(
                    panel.state.edit_add_widget,
                    StyleSetRequest::Display(display),
                );
            }
            OverlayEventData::OverlaysChanged(metas) => {
                panel.state.overlay_metas = metas;
                for (idx, btn) in panel.state.overlay_buttons.iter().enumerate() {
                    let display = if let Some(meta) = panel.state.overlay_metas.get(idx) {
                        btn.set_text(&mut com, Translation::from_raw_text(&meta.name));
                        //TODO: add category icons
                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    com.alterables
                        .set_style(btn.get_rect(), StyleSetRequest::Display(display));
                }
            }
            OverlayEventData::DevicesChanged => {
                log::info!("dev");
                for (i, (div, s)) in panel.state.devices.iter().enumerate() {
                    if let Some(dev) = app.input_state.devices.get(i)
                        && let Some(glyph) = panel.state.device_role_icons.get(dev.role)
                        && let Some(mut s) = panel.layout.state.widgets.get_as::<WidgetSprite>(*s)
                    {
                        log::info!("dev {i} ok");
                        s.params.glyph_data = Some(glyph.clone());
                        com.alterables
                            .set_style(*div, StyleSetRequest::Display(taffy::Display::Flex));
                    } else {
                        log::info!("dev {i} nok");
                        com.alterables
                            .set_style(*div, StyleSetRequest::Display(taffy::Display::None));
                    };
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
