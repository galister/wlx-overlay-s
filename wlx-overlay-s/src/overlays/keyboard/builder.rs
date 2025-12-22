use std::{collections::HashMap, rc::Rc};

use crate::{gui::panel::GuiPanel, state::AppState, subsystem::hid::XkbKeymap};
use glam::{FloatExt, Mat4, Vec2, Vec3, vec2};
use wgui::{
    animation::{Animation, AnimationEasing},
    assets::AssetPath,
    drawing::{self, Color},
    event::{self, CallbackMetadata, EventListenerKind},
    layout::LayoutParams,
    parser::Fetchable,
    renderer_vk::util,
    taffy::{self, prelude::length},
    widget::{
        EventResult,
        div::WidgetDiv,
        rectangle::{WidgetRectangle, WidgetRectangleParams},
        util::WLength,
    },
};

use super::{
    KeyButtonData, KeyState, KeyboardState, handle_press, handle_release,
    layout::{self, KeyCapType},
};

const BACKGROUND_PADDING: f32 = 16.0;
const PIXELS_PER_UNIT: f32 = 80.;

#[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
pub(super) fn create_keyboard_panel(
    app: &mut AppState,
    keymap: Option<&XkbKeymap>,
    state: KeyboardState,
    layout: &layout::Layout,
) -> anyhow::Result<GuiPanel<KeyboardState>> {
    let mut panel = GuiPanel::new_blank(app, state, Default::default())?;

    let globals = app.wgui_globals.clone();
    let accent_color = globals.get().defaults.accent_color;

    let anim_mult = globals.defaults().animation_mult;

    let (background, _) = panel.layout.add_child(
        panel.layout.content_root_widget,
        WidgetRectangle::create(WidgetRectangleParams {
            color: globals.defaults().bg_color,
            round: WLength::Units((16.0 * globals.defaults().rounding_mult).max(0.)),
            border: 2.0,
            border_color: accent_color,
            ..Default::default()
        }),
        taffy::Style {
            flex_direction: taffy::FlexDirection::Column,
            padding: length(BACKGROUND_PADDING),
            ..Default::default()
        },
    )?;

    let has_altgr = keymap.as_ref().is_some_and(|m| XkbKeymap::has_altgr(*m));

    let parse_doc_params = wgui::parser::ParseDocumentParams {
        globals,
        path: AssetPath::FileOrBuiltIn("gui/keyboard.xml"),
        extra: Default::default(),
    };

    let (_, mut gui_state_key) =
        wgui::parser::new_layout_from_assets(&parse_doc_params, &LayoutParams::default())?;

    for row in 0..layout.key_sizes.len() {
        let (div, _) = panel.layout.add_child(
            background.id,
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
            gui_state_key.instantiate_template(
                &parse_doc_params,
                &template_key,
                &mut panel.layout,
                div.id,
                params,
            )?;

            if let Ok(widget_id) = gui_state_key.get_widget_id(&my_id) {
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

                panel.add_event_listener(
                    widget_id,
                    EventListenerKind::MouseEnter,
                    Box::new({
                        let k = key_state.clone();
                        move |common, data, _app, _state| {
                            common.alterables.trigger_haptics();
                            on_enter_anim(k.clone(), common, data, accent_color, anim_mult);
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
                            on_leave_anim(k.clone(), common, data, accent_color, anim_mult);
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

    panel.layout.update(vec2(2048., 2048.), 0.0)?;
    panel.parser_state = gui_state_key;

    Ok(panel)
}

const BUTTON_HOVER_SCALE: f32 = 0.1;

fn get_anim_transform(pos: f32, widget_size: Vec2) -> Mat4 {
    util::centered_matrix(
        widget_size,
        &Mat4::from_scale(Vec3::splat(BUTTON_HOVER_SCALE.mul_add(pos, 1.0))),
    )
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
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        (10. * anim_mult) as _,
        AnimationEasing::OutBack,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, data.pos, accent_color);
            data.data.transform = get_anim_transform(data.pos, data.widget_boundary.size);
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
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        (15. * anim_mult) as _,
        AnimationEasing::OutQuad,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, 1.0 - data.pos, accent_color);
            data.data.transform = get_anim_transform(1.0 - data.pos, data.widget_boundary.size);
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
