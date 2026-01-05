use std::{collections::HashMap, rc::Rc, time::Duration};

use crate::{
    app_misc,
    gui::{
        panel::{GuiPanel, NewGuiPanelParams},
        timer::GuiTimer,
    },
    state::AppState,
    subsystem::hid::XkbKeymap,
    windowing::{backend::OverlayEventData, window::OverlayCategory},
};
use anyhow::Context;
use glam::{FloatExt, Mat4, Vec2, vec2, vec3};
use wgui::{
    animation::{Animation, AnimationEasing},
    assets::AssetPath,
    components::button::ComponentButton,
    drawing::{self, Color},
    event::{self, CallbackDataCommon, CallbackMetadata, EventAlterables, EventListenerKind},
    layout::LayoutUpdateParams,
    parser::{Fetchable, ParseDocumentParams},
    renderer_vk::util,
    taffy::{self, prelude::length},
    widget::{EventResult, div::WidgetDiv, rectangle::WidgetRectangle},
};

use super::{
    KeyButtonData, KeyState, KeyboardState, handle_press, handle_release,
    layout::{self, KeyCapType},
};

const PIXELS_PER_UNIT: f32 = 80.;

fn new_doc_params(panel: &mut GuiPanel<KeyboardState>) -> ParseDocumentParams<'static> {
    ParseDocumentParams {
        globals: panel.layout.state.globals.clone(),
        path: AssetPath::FileOrBuiltIn("gui/keyboard.xml"),
        extra: panel.doc_extra.take().unwrap_or_default(),
    }
}

#[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
pub(super) fn create_keyboard_panel(
    app: &mut AppState,
    keymap: Option<&XkbKeymap>,
    state: KeyboardState,
    layout: &layout::Layout,
) -> anyhow::Result<GuiPanel<KeyboardState>> {
    let mut panel =
        GuiPanel::new_from_template(app, "gui/keyboard.xml", state, NewGuiPanelParams::default())?;

    let doc_params = new_doc_params(&mut panel);

    let globals = app.wgui_globals.clone();
    let accent_color = globals.get().defaults.accent_color;

    let anim_mult = globals.defaults().animation_mult;

    let root = panel
        .parser_state
        .get_widget_id("keyboard_root")
        .context("Element with id 'keyboard_root' not found; keyboard.xml may be out of date.")?;

    let has_altgr = keymap.as_ref().is_some_and(|m| XkbKeymap::has_altgr(m));

    for row in 0..layout.key_sizes.len() {
        let (div, _) = panel.layout.add_child(
            root,
            WidgetDiv::create(),
            taffy::Style {
                flex_direction: taffy::FlexDirection::Row,
                ..Default::default()
            },
        )?;

        for col in 0..layout.key_sizes[row].len() {
            let my_size_f32 = layout.key_sizes[row][col];

            let key_width = PIXELS_PER_UNIT * my_size_f32;
            let key_height = PIXELS_PER_UNIT;

            let taffy_size = taffy::Size {
                width: length(key_width),
                height: length(PIXELS_PER_UNIT),
            };

            let Some(key) = layout.get_key_data(keymap, has_altgr, col, row) else {
                let _ = panel.layout.add_child(
                    div.id,
                    WidgetDiv::create(),
                    taffy::Style {
                        size: taffy_size,
                        min_size: taffy_size,
                        max_size: taffy_size,
                        ..Default::default()
                    },
                )?;
                continue;
            };

            let my_id: Rc<str> = Rc::from(format!("key-{row}-{col}"));

            let my_modifier = match key.button_state {
                KeyButtonData::Modifier { modifier, .. } => Some(modifier),
                _ => None,
            };

            // todo: make this easier to maintain somehow
            let mut params: HashMap<Rc<str>, Rc<str>> = HashMap::new();
            params.insert(Rc::from("id"), my_id.clone());
            params.insert(Rc::from("width"), Rc::from(key_width.to_string()));
            params.insert(Rc::from("height"), Rc::from(key_height.to_string()));

            let mut label = key.label.into_iter();
            label
                .next()
                .and_then(|s| params.insert("text".into(), s.into()));

            match key.cap_type {
                KeyCapType::LetterAltGr => {
                    label
                        .next()
                        .and_then(|s| params.insert("text_altgr".into(), s.into()));
                }
                KeyCapType::Symbol => {
                    label
                        .next()
                        .and_then(|s| params.insert("text_shift".into(), s.into()));
                }
                KeyCapType::SymbolAltGr => {
                    label
                        .next()
                        .and_then(|s| params.insert("text_shift".into(), s.into()));
                    label
                        .next()
                        .and_then(|s| params.insert("text_altgr".into(), s.into()));
                }
                _ => {}
            }

            let template_key = format!("Key{:?}", key.cap_type);
            panel.parser_state.instantiate_template(
                &doc_params,
                &template_key,
                &mut panel.layout,
                div.id,
                params,
            )?;

            if let Ok(widget_id) = panel.parser_state.get_widget_id(&my_id) {
                let key_state = {
                    let rect = panel
                        .layout
                        .state
                        .widgets
                        .get_as::<WidgetRectangle>(widget_id)
                        .unwrap(); // want panic

                    Rc::new(KeyState {
                        button_state: key.button_state,
                        color: rect.params.color,
                        color2: rect.params.color2,
                        border_color: rect.params.border_color,
                        border: rect.params.border,
                        drawn_state: false.into(),
                    })
                };

                let width_mul = 1. / my_size_f32;

                panel.add_event_listener(
                    widget_id,
                    EventListenerKind::MouseEnter,
                    Box::new({
                        let k = key_state.clone();
                        move |common, data, _app, _state| {
                            common.alterables.trigger_haptics();
                            on_enter_anim(
                                k.clone(),
                                common,
                                data,
                                accent_color,
                                anim_mult,
                                width_mul,
                            );
                            Ok(EventResult::Pass)
                        }
                    }),
                );
                panel.add_event_listener(
                    widget_id,
                    EventListenerKind::MouseLeave,
                    Box::new({
                        let k = key_state.clone();
                        move |common, data, _app, _state| {
                            common.alterables.trigger_haptics();
                            on_leave_anim(
                                k.clone(),
                                common,
                                data,
                                accent_color,
                                anim_mult,
                                width_mul,
                            );
                            Ok(EventResult::Pass)
                        }
                    }),
                );
                panel.add_event_listener(
                    widget_id,
                    EventListenerKind::MousePress,
                    Box::new({
                        let k = key_state.clone();
                        move |common, data, app, state| {
                            let CallbackMetadata::MouseButton(button) = data.metadata else {
                                panic!("CallbackMetadata should contain MouseButton!");
                            };

                            handle_press(app, &k, state, button);
                            on_press_anim(k.clone(), common, data);
                            Ok(EventResult::Pass)
                        }
                    }),
                );
                panel.add_event_listener(
                    widget_id,
                    EventListenerKind::MouseRelease,
                    Box::new({
                        let k = key_state.clone();
                        move |common, data, app, state| {
                            if handle_release(app, &k, state) {
                                on_release_anim(k.clone(), common, data);
                            }
                            Ok(EventResult::Pass)
                        }
                    }),
                );

                if let Some(modifier) = my_modifier {
                    panel.add_event_listener(
                        widget_id,
                        EventListenerKind::InternalStateChange,
                        Box::new({
                            let k = key_state.clone();
                            move |common, data, _app, state| {
                                if (state.modifiers & modifier) != 0 {
                                    on_press_anim(k.clone(), common, data);
                                } else {
                                    on_release_anim(k.clone(), common, data);
                                }
                                Ok(EventResult::Pass)
                            }
                        }),
                    );
                }
            } else {
                log::warn!("No ID for key at ({row}, {col})");
            }
        }
    }

    panel.on_notify = Some(Box::new(move |panel, app, event_data| {
        let mut alterables = EventAlterables::default();

        match event_data {
            OverlayEventData::ActiveSetChanged(current_set) => {
                let mut com = CallbackDataCommon {
                    alterables: &mut alterables,
                    state: &panel.layout.state,
                };
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
                let sets_root = panel.parser_state.get_widget_id("sets_root")?;
                panel.layout.remove_children(sets_root);
                panel.state.set_buttons.clear();

                for i in 0..num_sets {
                    let mut params = HashMap::new();
                    params.insert("idx".into(), i.to_string().into());
                    params.insert("display".into(), (i + 1).to_string().into());
                    panel.parser_state.instantiate_template(
                        &doc_params,
                        "Set",
                        &mut panel.layout,
                        sets_root,
                        params,
                    )?;
                    let set_button = panel
                        .parser_state
                        .fetch_component_as::<ComponentButton>(&format!("set_{i}"))?;
                    if panel.state.current_set == Some(i) {
                        let mut com = CallbackDataCommon {
                            alterables: &mut alterables,
                            state: &panel.layout.state,
                        };
                        set_button.set_sticky_state(&mut com, true);
                    }
                    panel.state.set_buttons.push(set_button);
                }
                panel.process_custom_elems(app);
            }
            OverlayEventData::OverlaysChanged(metas) => {
                let panels_root = panel.parser_state.get_widget_id("panels_root")?;
                let apps_root = panel.parser_state.get_widget_id("apps_root")?;
                panel.layout.remove_children(panels_root);
                panel.layout.remove_children(apps_root);
                panel.state.overlay_buttons.clear();

                for (i, meta) in metas.iter().enumerate() {
                    let mut params = HashMap::new();

                    let (template, root) = match meta.category {
                        OverlayCategory::Screen => {
                            params.insert(
                                "display".into(),
                                format!(
                                    "{}{}",
                                    (*meta.name).chars().next().unwrap_or_default(),
                                    (*meta.name).chars().last().unwrap_or_default()
                                )
                                .into(),
                            );
                            ("Screen", panels_root)
                        }
                        OverlayCategory::Mirror => {
                            params.insert("display".into(), meta.name.as_ref().into());
                            ("Mirror", panels_root)
                        }
                        OverlayCategory::Panel => ("Panel", panels_root),
                        OverlayCategory::WayVR => {
                            params.insert(
                                "icon".into(),
                                meta.icon
                                    .as_ref()
                                    .expect("WayVR overlay without Icon attribute!")
                                    .as_ref()
                                    .into(),
                            );
                            ("App", apps_root)
                        }
                        OverlayCategory::Dashboard => {
                            let overlay_button = panel
                                .parser_state
                                .fetch_component_as::<ComponentButton>("btn_dashboard")?;

                            log::error!("Found dashboard at: {:?}", meta.id);

                            if meta.visible {
                                let mut com = CallbackDataCommon {
                                    alterables: &mut alterables,
                                    state: &panel.layout.state,
                                };
                                overlay_button.set_sticky_state(&mut com, true);
                            }
                            panel.state.overlay_buttons.insert(meta.id, overlay_button);
                            continue;
                        }
                        _ => continue,
                    };

                    params.insert("idx".into(), i.to_string().into());
                    params.insert("name".into(), meta.name.as_ref().into());
                    panel.parser_state.instantiate_template(
                        &doc_params,
                        template,
                        &mut panel.layout,
                        root,
                        params,
                    )?;
                    let overlay_button = panel
                        .parser_state
                        .fetch_component_as::<ComponentButton>(&format!("overlay_{i}"))?;
                    if meta.visible {
                        let mut com = CallbackDataCommon {
                            alterables: &mut alterables,
                            state: &panel.layout.state,
                        };
                        overlay_button.set_sticky_state(&mut com, true);
                    }
                    panel.state.overlay_buttons.insert(meta.id, overlay_button);
                }
                panel.process_custom_elems(app);
            }
            OverlayEventData::VisibleOverlaysChanged(overlays) => {
                let mut com = CallbackDataCommon {
                    alterables: &mut alterables,
                    state: &panel.layout.state,
                };
                let mut overlay_buttons = panel.state.overlay_buttons.clone();

                for visible in &*overlays {
                    if let Some(btn) = overlay_buttons.remove(*visible) {
                        btn.set_sticky_state(&mut com, true);
                    }
                }

                for btn in overlay_buttons.values() {
                    btn.set_sticky_state(&mut com, false);
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

    app_misc::process_layout_result(
        app,
        panel.layout.update(&mut LayoutUpdateParams {
            size: vec2(2048., 2048.),
            timestep_alpha: 0.0,
        })?,
    );

    Ok(panel)
}

const BUTTON_HOVER_SCALE: f32 = 0.1;

fn get_anim_transform(pos: f32, widget_size: Vec2, width_mult: f32) -> Mat4 {
    let scale = vec3(
        (BUTTON_HOVER_SCALE * width_mult).mul_add(pos, 1.0),
        BUTTON_HOVER_SCALE.mul_add(pos, 1.0),
        1.0,
    );

    util::centered_matrix(widget_size, &Mat4::from_scale(scale))
}

fn set_anim_color(
    key_state: &KeyState,
    rect: &mut WidgetRectangle,
    pos: f32,
    accent_color: drawing::Color,
) {
    // fade to accent color
    rect.params.color.r = key_state.color.r.lerp(accent_color.r, pos);
    rect.params.color.g = key_state.color.g.lerp(accent_color.g, pos);
    rect.params.color.b = key_state.color.b.lerp(accent_color.b, pos);

    // fade to accent color
    rect.params.color2.r = key_state.color2.r.lerp(accent_color.r, pos);
    rect.params.color2.g = key_state.color2.g.lerp(accent_color.g, pos);
    rect.params.color2.b = key_state.color2.b.lerp(accent_color.b, pos);

    // fade to white
    rect.params.border_color.r = key_state.border_color.r.lerp(1.0, pos);
    rect.params.border_color.g = key_state.border_color.g.lerp(1.0, pos);
    rect.params.border_color.b = key_state.border_color.b.lerp(1.0, pos);
    rect.params.border_color.a = key_state.border_color.a.lerp(1.0, pos);

    rect.params.border = key_state.border.lerp(key_state.border * 1.5, pos);
}

fn on_enter_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &event::CallbackData,
    accent_color: drawing::Color,
    anim_mult: f32,
    width_mult: f32,
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        (10. * anim_mult) as _,
        AnimationEasing::OutBack,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, data.pos, accent_color);
            data.data.transform =
                get_anim_transform(data.pos, data.widget_boundary.size, width_mult);
            common.alterables.mark_redraw();
        }),
    ));
}

fn on_leave_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &event::CallbackData,
    accent_color: drawing::Color,
    anim_mult: f32,
    width_mult: f32,
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        (15. * anim_mult) as _,
        AnimationEasing::OutQuad,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, 1.0 - data.pos, accent_color);
            data.data.transform =
                get_anim_transform(1.0 - data.pos, data.widget_boundary.size, width_mult);
            common.alterables.mark_redraw();
        }),
    ));
}

fn on_press_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    if key_state.drawn_state.get() {
        return;
    }
    let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
    rect.params.border_color = Color::new(1.0, 1.0, 1.0, 1.0);
    common.alterables.mark_redraw();
    key_state.drawn_state.set(true);
}

fn on_release_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &mut event::CallbackData,
) {
    if !key_state.drawn_state.get() {
        return;
    }
    let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
    rect.params.border_color = key_state.border_color;
    common.alterables.mark_redraw();
    key_state.drawn_state.set(false);
}
