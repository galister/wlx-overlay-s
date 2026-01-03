use std::{collections::HashMap, rc::Rc, time::Duration};

use glam::{Affine3A, Quat, Vec3, Vec3A, vec3};
use idmap::DirectIdMap;
use slotmap::SecondaryMap;
use wgui::{
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables, EventCallback, StyleSetRequest},
    i18n::Translation,
    layout::WidgetID,
    parser::Fetchable,
    renderer_vk::text::custom_glyph::CustomGlyphData,
    taffy,
    widget::{EventResult, label::WidgetLabel, sprite::WidgetSprite},
};
use wlx_common::{
    common::LeftRight,
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    backend::{
        input::TrackedDeviceRole,
        task::{OverlayTask, TaskType},
    },
    gui::{
        panel::{GuiPanel, NewGuiPanelParams, OnCustomAttribFunc, button::BUTTON_EVENTS},
        timer::GuiTimer,
    },
    state::AppState,
    windowing::{
        OverlayID, OverlaySelector, Z_ORDER_WATCH,
        backend::{OverlayEventData, OverlayMeta},
        manager::MAX_OVERLAY_SETS,
        window::{OverlayCategory, OverlayWindowConfig, OverlayWindowData},
    },
};

pub const WATCH_NAME: &str = "watch";
const MAX_TOOLBOX_BUTTONS: usize = 16;
const MAX_DEVICES: usize = 12;

pub const WATCH_POS: Vec3 = vec3(-0.03, -0.01, 0.125);
pub const WATCH_ROT: Quat = Quat::from_xyzw(-0.707_106_6, 0.000_796_361_8, 0.707_106_6, 0.0);

struct OverlayButton {
    button: Rc<ComponentButton>,
    label: WidgetID,
    sprite: WidgetID,
    condensed: bool,
}

#[derive(Default)]
struct WatchState {
    current_set: Option<usize>,
    set_buttons: Vec<Rc<ComponentButton>>,
    overlay_buttons: Vec<OverlayButton>,
    overlay_metas: Vec<OverlayMeta>,
    overlay_indices: SecondaryMap<OverlayID, usize>,
    edit_mode_widgets: Vec<(WidgetID, bool)>,
    edit_add_widget: WidgetID,
    device_role_icons: DirectIdMap<TrackedDeviceRole, CustomGlyphData>,
    overlay_cat_icons: DirectIdMap<OverlayCategory, CustomGlyphData>,
    devices: Vec<(WidgetID, WidgetID)>,
    keyboard_oid: OverlayID,
    dashboard_oid: OverlayID,
    num_sets: usize,
}

#[allow(clippy::significant_drop_tightening)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub fn create_watch(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let state = WatchState::default();

    let on_custom_attrib: OnCustomAttribFunc = Box::new(move |layout, parser, attribs, _app| {
        let Ok(button) =
            parser.fetch_component_from_widget_id_as::<ComponentButton>(attribs.widget_id)
        else {
            return;
        };

        for (name, kind, test_button, test_duration) in &BUTTON_EVENTS {
            let Some(action) = attribs.get_value(name) else {
                continue;
            };

            let mut args = action.split_whitespace();
            let Some(command) = args.next() else {
                continue;
            };

            let button = button.clone();

            let callback: EventCallback<AppState, WatchState> = match command {
                "::EditModeDeleteSet" => Box::new(move |_common, data, app, _state| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    app.tasks
                        .enqueue(TaskType::Overlay(OverlayTask::DeleteActiveSet));
                    Ok(EventResult::Consumed)
                }),
                "::EditModeAddSet" => Box::new(move |_common, data, app, _state| {
                    if !test_button(data) || !test_duration(&button, app) {
                        return Ok(EventResult::Pass);
                    }

                    app.tasks.enqueue(TaskType::Overlay(OverlayTask::AddSet));
                    Ok(EventResult::Consumed)
                }),
                "::EditModeOverlayToggle" => {
                    let arg = args.next().unwrap_or_default();
                    let Ok(idx) = arg.parse::<usize>() else {
                        log::error!("{command} has invalid argument: \"{arg}\"");
                        return;
                    };
                    Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        let Some(overlay) = state.overlay_metas.get(idx) else {
                            log::error!("No overlay at index {idx}.");
                            return Ok(EventResult::Consumed);
                        };

                        app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
                            OverlaySelector::Id(overlay.id),
                            Box::new(move |app, owc| {
                                if owc.active_state.is_none() {
                                    owc.activate(app);
                                } else {
                                    owc.deactivate();
                                }
                            }),
                        )));
                        Ok(EventResult::Consumed)
                    })
                }
                "::SingleSetOverlayToggle" => {
                    let arg = args.next().unwrap_or_default();
                    let Ok(idx) = arg.parse::<usize>() else {
                        log::error!("{command} has invalid argument: \"{arg}\"");
                        return;
                    };
                    Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        let Some(overlay) = state.overlay_metas.get(idx) else {
                            log::error!("No overlay at index {idx}.");
                            return Ok(EventResult::Consumed);
                        };

                        app.tasks
                            .enqueue(TaskType::Overlay(OverlayTask::SoftToggleOverlay(
                                OverlaySelector::Id(overlay.id),
                            )));
                        Ok(EventResult::Consumed)
                    })
                }
                "::SingleSetOverlayReset" => {
                    let arg = args.next().unwrap_or_default();
                    let Ok(idx) = arg.parse::<usize>() else {
                        log::error!("{command} has invalid argument: \"{arg}\"");
                        return;
                    };
                    Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        let Some(overlay) = state.overlay_metas.get(idx) else {
                            log::error!("No overlay at index {idx}.");
                            return Ok(EventResult::Consumed);
                        };

                        app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
                            OverlaySelector::Id(overlay.id),
                            Box::new(|app, owc| owc.activate(app)),
                        )));
                        Ok(EventResult::Consumed)
                    })
                }
                _ => return,
            };

            let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
            log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
        }
    });

    let watch_xml = if app.session.config.single_set_mode {
        "gui/watch-noset.xml"
    } else {
        "gui/watch.xml"
    };

    let mut panel = GuiPanel::new_from_template(
        app,
        watch_xml,
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
                    } else if &*id == "toolbox" || &*id == "toolbox-condensed" {
                        for idx in 0..MAX_TOOLBOX_BUTTONS {
                            let id_str = format!("overlay_{idx}");

                            let button = if let Ok(button) =
                                parser_state.fetch_component_as::<ComponentButton>(&id_str)
                            {
                                button
                            } else {
                                let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
                                params.insert("idx".into(), idx.to_string().into());
                                parser_state.instantiate_template(
                                    doc_params, "Overlay", layout, widget, params,
                                )?;
                                parser_state.fetch_component_as::<ComponentButton>(&id_str)?
                            };

                            state.overlay_buttons.push(OverlayButton {
                                button,
                                label: parser_state
                                    .get_widget_id(&format!("overlay_{idx}_label"))
                                    .inspect_err(|e| log::warn!("{e:?}"))
                                    .unwrap_or_default(),
                                sprite: parser_state
                                    .get_widget_id(&format!("overlay_{idx}_sprite"))
                                    .inspect_err(|e| log::warn!("{e:?}"))
                                    .unwrap_or_default(),
                                condensed: id.ends_with("-condensed"),
                            });
                        }
                    } else if id.starts_with("overlay_") && id.ends_with("_sprite") {
                        // store device icons from xml
                        let id_n = id
                            .replace("overlay_", "")
                            .replace("_sprite", "")
                            .parse::<u64>()?;

                        let category = match id_n {
                            0 => OverlayCategory::Panel,
                            1 => OverlayCategory::Screen,
                            2 => OverlayCategory::Mirror,
                            3 => OverlayCategory::WayVR,
                            _ => return Ok(()), // not parsing the first 4 elems
                        };

                        let sprite = layout
                            .state
                            .widgets
                            .get_as::<WidgetSprite>(widget)
                            .ok_or_else(|| {
                                anyhow::anyhow!("{id} is expected to be a sprite, but it isn't.")
                            })?;

                        let src = sprite.get_content().ok_or_else(|| {
                            anyhow::anyhow!("{id} is expected to have a src, but it doesn't.")
                        })?;

                        state.overlay_cat_icons.insert(category, src);
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

                        let src = sprite.get_content().ok_or_else(|| {
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
                                params.insert("src".into(), String::new().into());
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

    let btn_edit_mode = panel
        .parser_state
        .fetch_component_as::<ComponentButton>("btn_edit_mode")
        .ok();
    let btn_keyboard = panel
        .parser_state
        .fetch_component_as::<ComponentButton>("btn_keyboard")
        .ok();
    let btn_dashboard = panel
        .parser_state
        .fetch_component_as::<ComponentButton>("btn_dashboard")
        .ok();

    panel.on_notify = Some(Box::new(move |panel, app, event_data| {
        let mut alterables = EventAlterables::default();
        let mut com = CallbackDataCommon {
            alterables: &mut alterables,
            state: &panel.layout.state,
        };

        match event_data {
            OverlayEventData::ActiveSetChanged(current_set) => {
                if let Some(old_set) = panel.state.current_set.take()
                    && let Some(old_set) = panel.state.set_buttons.get_mut(old_set)
                {
                    old_set.set_sticky_state(&mut com, false);
                }
                if let Some(new_set) = current_set
                    && let Some(new_set) = panel.state.set_buttons.get_mut(new_set)
                {
                    new_set.set_sticky_state(&mut com, true);
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

                if let Some(btn) = btn_edit_mode.as_ref() {
                    btn.set_sticky_state(&mut com, edit_mode);
                }
            }
            OverlayEventData::OverlaysChanged(metas) => {
                panel.state.overlay_metas.clear();
                for meta in metas {
                    match meta.category {
                        OverlayCategory::Keyboard => {
                            panel.state.keyboard_oid = meta.id;
                            if let Some(btn_keyboard) = btn_keyboard.as_ref() {
                                btn_keyboard.set_sticky_state(&mut com, meta.visible);
                            }
                        }
                        OverlayCategory::Dashboard => {
                            if let Some(btn_dashboard) = btn_dashboard.as_ref() {
                                btn_dashboard.set_sticky_state(&mut com, meta.visible);
                            }
                            panel.state.dashboard_oid = meta.id;
                        }
                        OverlayCategory::Internal => {}
                        _ => panel.state.overlay_metas.push(meta),
                    }
                }

                panel.state.overlay_indices.clear();
                for (idx, meta) in panel.state.overlay_metas.iter().enumerate() {
                    panel.state.overlay_indices.insert(meta.id, idx);
                }

                for (idx, btn) in panel.state.overlay_buttons.iter().enumerate() {
                    let display = if let Some(meta) = panel.state.overlay_metas.get(idx) {
                        let name = if btn.condensed {
                            condense_overlay_name(&meta.name)
                        } else {
                            sanitize_overlay_name(&meta.name)
                        };

                        if let Some(mut label) =
                            panel.layout.state.widgets.get_as::<WidgetLabel>(btn.label)
                        {
                            label.set_text(&mut com, Translation::from_raw_text_rc(name));
                        } else {
                            btn.button
                                .set_text(&mut com, Translation::from_raw_text_rc(name));
                        }

                        if let Some(mut sprite) = panel
                            .layout
                            .state
                            .widgets
                            .get_as::<WidgetSprite>(btn.sprite)
                            && let Some(glyph) = panel.state.overlay_cat_icons.get(meta.category)
                        {
                            sprite.set_content(&mut com, Some(glyph.clone()));
                        }

                        btn.button.set_sticky_state(&mut com, meta.visible);

                        taffy::Display::Flex
                    } else {
                        taffy::Display::None
                    };
                    com.alterables
                        .set_style(btn.button.get_rect(), StyleSetRequest::Display(display));
                }
            }
            OverlayEventData::VisibleOverlaysChanged(overlays) => {
                for meta in &mut panel.state.overlay_metas {
                    meta.visible = false;
                }

                let mut keyboard_visible = false;
                let mut dashboard_visible = false;

                for visible in &overlays {
                    if let Some(idx) = panel.state.overlay_indices.get(*visible)
                        && let Some(o) = panel.state.overlay_metas.get_mut(*idx)
                    {
                        o.visible = true;
                    } else if panel.state.keyboard_oid == *visible {
                        keyboard_visible = true;
                    } else if panel.state.dashboard_oid == *visible {
                        dashboard_visible = true;
                    }
                }

                for (idx, btn) in panel.state.overlay_buttons.iter().enumerate() {
                    let Some(meta) = panel.state.overlay_metas.get(idx) else {
                        continue;
                    };
                    btn.button.set_sticky_state(&mut com, meta.visible);
                }
                if let Some(btn_keyboard) = btn_keyboard.as_ref() {
                    btn_keyboard.set_sticky_state(&mut com, keyboard_visible);
                }
                if let Some(btn_dashboard) = btn_dashboard.as_ref() {
                    btn_dashboard.set_sticky_state(&mut com, dashboard_visible);
                }
            }
            OverlayEventData::DevicesChanged => {
                for (i, (div, s)) in panel.state.devices.iter().enumerate() {
                    if let Some(dev) = app.input_state.devices.get(i)
                        && let Some(glyph) = panel.state.device_role_icons.get(dev.role)
                        && let Some(mut s) = panel.layout.state.widgets.get_as::<WidgetSprite>(*s)
                    {
                        s.set_content(&mut com, Some(glyph.clone()));
                        com.alterables
                            .set_style(*div, StyleSetRequest::Display(taffy::Display::Flex));
                    } else {
                        com.alterables
                            .set_style(*div, StyleSetRequest::Display(taffy::Display::None));
                    }
                }
            }
            _ => {}
        }

        panel.layout.process_alterables(alterables)?;
        Ok(())
    }));

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: LeftRight::Left,
        lerp: 1.0,
        align_to_hmd: false,
    };

    panel.update_layout(app)?;

    Ok(OverlayWindowConfig {
        name: WATCH_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                WATCH_ROT,
                WATCH_POS,
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

fn sanitize_overlay_name(str: &str) -> Rc<str> {
    str.replace("-wvr", "").into()
}

fn condense_overlay_name(str: &str) -> Rc<str> {
    str.replace("DP-", "D")
        .replace("HDMI-A-", "H")
        .replace("WVR-wvr_", "W")
        .replace("WVR-wvr", "W0")
        .replace("Keyboard", "")
        .into()
}
