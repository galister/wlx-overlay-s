use std::{collections::HashMap, rc::Rc};

use glam::{vec2, vec3, Affine3A, Mat4, Quat, Vec2, Vec3};
use wgui::{
    animation::{Animation, AnimationEasing},
    assets::AssetPath,
    drawing::Color,
    event::{self, CallbackMetadata, EventListenerKind},
    layout::LayoutParams,
    parser::Fetchable,
    renderer_vk::util,
    taffy::{self, prelude::length},
    widget::{
        div::WidgetDiv,
        rectangle::{WidgetRectangle, WidgetRectangleParams},
        util::WLength,
        EventResult,
    },
};

use crate::{
    gui::panel::GuiPanel,
    state::AppState,
    subsystem::hid::{XkbKeymap, ALT, CTRL, META, SHIFT, SUPER},
    windowing::window::{OverlayWindowConfig, OverlayWindowState, Positioning},
};

use super::{
    handle_press, handle_release,
    layout::{self, AltModifier, KeyCapType},
    KeyButtonData, KeyState, KeyboardBackend, KeyboardState, KEYBOARD_NAME,
};

const BACKGROUND_PADDING: f32 = 4.;
const PIXELS_PER_UNIT: f32 = 80.;

#[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
pub fn create_keyboard(
    app: &mut AppState,
    mut keymap: Option<XkbKeymap>,
) -> anyhow::Result<OverlayWindowConfig> {
    let layout = layout::Layout::load_from_disk();
    let state = KeyboardState {
        modifiers: 0,
        alt_modifier: match layout.alt_modifier {
            AltModifier::Shift => SHIFT,
            AltModifier::Ctrl => CTRL,
            AltModifier::Alt => ALT,
            AltModifier::Super => SUPER,
            AltModifier::Meta => META,
            _ => 0,
        },
        processes: vec![],
    };

    let mut panel = GuiPanel::new_blank(app, state, false)?;

    let (background, _) = panel.layout.add_child(
        panel.layout.content_root_widget,
        WidgetRectangle::create(WidgetRectangleParams {
            color: wgui::drawing::Color::new(0., 0., 0., 0.6),
            round: WLength::Units(4.0),
            ..Default::default()
        }),
        taffy::Style {
            flex_direction: taffy::FlexDirection::Column,
            padding: length(BACKGROUND_PADDING),
            ..Default::default()
        },
    )?;

    let has_altgr = keymap.as_ref().is_some_and(XkbKeymap::has_altgr);

    if !layout.auto_labels.unwrap_or(true) {
        keymap = None;
    }

    let parse_doc_params = wgui::parser::ParseDocumentParams {
        globals: app.wgui_globals.clone(),
        path: AssetPath::BuiltIn("gui/keyboard.xml"),
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

            let Some(key) = layout.get_key_data(keymap.as_ref(), has_altgr, col, row) else {
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
                            on_enter_anim(k.clone(), common, data);
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
                            on_leave_anim(k.clone(), common, data);
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

    let width = layout.row_size * 0.05 * app.session.config.keyboard_scale;

    Ok(OverlayWindowConfig {
        name: KEYBOARD_NAME.into(),
        default_state: OverlayWindowState {
            grabbable: true,
            positioning: Positioning::Anchored,
            interactable: true,
            curvature: Some(0.15),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * width,
                Quat::from_rotation_x(-10f32.to_radians()),
                vec3(0.0, -0.5, -0.5),
            ),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(KeyboardBackend { panel }))
    })
}

const BUTTON_HOVER_SCALE: f32 = 0.1;

fn get_anim_transform(pos: f32, widget_size: Vec2) -> Mat4 {
    util::centered_matrix(
        widget_size,
        &Mat4::from_scale(Vec3::splat(BUTTON_HOVER_SCALE.mul_add(pos, 1.0))),
    )
}

fn set_anim_color(key_state: &KeyState, rect: &mut WidgetRectangle, pos: f32) {
    let br1 = pos * 0.25;
    let br2 = pos * 0.15;

    rect.params.color.r = key_state.color.r + br1;
    rect.params.color.g = key_state.color.g + br1;
    rect.params.color.b = key_state.color.b + br1;

    rect.params.color2.r = key_state.color2.r + br2;
    rect.params.color2.g = key_state.color2.g + br2;
    rect.params.color2.b = key_state.color2.b + br2;
}

fn on_enter_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &event::CallbackData,
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        10,
        AnimationEasing::OutBack,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, data.pos);
            data.data.transform = get_anim_transform(data.pos, data.widget_boundary.size);
            common.alterables.mark_redraw();
        }),
    ));
}

fn on_leave_anim(
    key_state: Rc<KeyState>,
    common: &mut event::CallbackDataCommon,
    data: &event::CallbackData,
) {
    common.alterables.animate(Animation::new(
        data.widget_id,
        15,
        AnimationEasing::OutQuad,
        Box::new(move |common, data| {
            let rect = data.obj.get_as_mut::<WidgetRectangle>().unwrap();
            set_anim_color(&key_state, rect, 1.0 - data.pos);
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
