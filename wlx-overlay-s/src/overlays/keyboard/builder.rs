use std::{cell::RefCell, collections::HashMap, rc::Rc};

use glam::{Affine2, Mat4, Vec2, Vec3, vec2, vec3a};
use wgui::{
    animation::{Animation, AnimationEasing},
    drawing::Color,
    event::{self, EventListener},
    renderer_vk::util,
    taffy::{self, prelude::length},
    widget::{
        div::Div,
        rectangle::{Rectangle, RectangleParams},
        util::WLength,
    },
};

use crate::{
    backend::overlay::{OverlayData, OverlayState, Positioning},
    gui::{self, panel::GuiPanel},
    state::AppState,
    subsystem::hid::{ALT, CTRL, META, SHIFT, SUPER, XkbKeymap},
};

use super::{
    KEYBOARD_NAME, KeyButtonData, KeyState, KeyboardBackend, KeyboardState,
    layout::{self, AltModifier, KeyCapType},
};

const BACKGROUND_PADDING: f32 = 4.;
const PIXELS_PER_UNIT: f32 = 80.;

#[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
pub fn create_keyboard<O>(
    app: &AppState,
    mut keymap: Option<XkbKeymap>,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let layout = layout::Layout::load_from_disk();
    let state = Rc::new(RefCell::new(KeyboardState {
        invoke_action: None,
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
    }));

    let mut panel = GuiPanel::new_blank(app)?;

    let (background, _) = panel.layout.add_child(
        panel.layout.root_widget,
        Rectangle::create(RectangleParams {
            color: wgui::drawing::Color::new(0., 0., 0., 0.6),
            round: WLength::Units(4.0),
            ..Default::default()
        })
        .unwrap(),
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

    let (_, mut gui_state_key) = wgui::parser::new_layout_from_assets(
        Box::new(gui::asset::GuiAsset {}),
        "gui/keyboard.xml",
    )?;

    for row in 0..layout.key_sizes.len() {
        let (div, _) = panel.layout.add_child(
            background,
            Div::create().unwrap(),
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
                    div,
                    Div::create()?,
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
            gui_state_key.process_template(&template_key, &mut panel.layout, div, params)?;

            if let Some(widget_id) = gui_state_key.ids.get(&*my_id) {
                let key_state = {
                    let widget = panel
                        .layout
                        .widget_map
                        .get(*widget_id)
                        .unwrap() // want panic
                        .lock()
                        .unwrap(); // want panic

                    let rect = widget.obj.get_as::<Rectangle>();

                    Rc::new(KeyState {
                        button_state: key.button_state,
                        color: rect.params.color,
                        color2: rect.params.color2,
                        border_color: rect.params.border_color,
                        drawn_state: false.into(),
                    })
                };

                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseEnter(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, ()| {
                            data.trigger_haptics = true;
                            on_enter_anim(k.clone(), kb.clone(), data);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseLeave(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, ()| {
                            data.trigger_haptics = true;
                            on_leave_anim(k.clone(), kb.clone(), data);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MousePress(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, button| {
                            kb.borrow_mut().invoke_action = Some(super::InvokeAction {
                                key: k.clone(),
                                button,
                                pressed: true,
                            });
                            on_press_anim(k.clone(), data);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseRelease(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, button| {
                            kb.borrow_mut().invoke_action = Some(super::InvokeAction {
                                key: k.clone(),
                                button,
                                pressed: false,
                            });
                            if !matches!(&k.button_state, KeyButtonData::Modifier { sticky, .. } if sticky.get()) {
                                on_release_anim(k.clone(), data);
                            }
                        }
                    })),
                );

                if let Some(modifier) = my_modifier {
                    panel.layout.add_event_listener(
                        *widget_id,
                        EventListener::InternalStateChange(Box::new({
                            let (k, kb) = (key_state.clone(), state.clone());
                            move |data, _| {
                                if (kb.borrow().modifiers & modifier) != 0 {
                                    on_press_anim(k.clone(), data);
                                } else {
                                    on_release_anim(k.clone(), data);
                                }
                            }
                        })),
                    );
                }
            } else {
                log::warn!("No ID for key at ({row}, {col})");
            }
        }
    }

    panel.layout.update(vec2(2048., 2048.), 0.0)?;

    let interaction_transform = Affine2::from_translation(vec2(0.5, 0.5))
        * Affine2::from_scale(vec2(
            1.,
            -panel.layout.content_size.x / panel.layout.content_size.y,
        ));

    let width = layout.row_size * 0.05 * app.session.config.keyboard_scale;

    Ok(OverlayData {
        state: OverlayState {
            name: KEYBOARD_NAME.into(),
            grabbable: true,
            recenter: true,
            positioning: Positioning::Anchored,
            interactable: true,
            spawn_scale: width,
            spawn_point: vec3a(0., -0.5, 0.),
            interaction_transform,
            ..Default::default()
        },
        backend: Box::new(KeyboardBackend { panel, state }),
        ..Default::default()
    })
}

const BUTTON_HOVER_SCALE: f32 = 0.1;

fn get_anim_transform(pos: f32, widget_size: Vec2) -> Mat4 {
    util::centered_matrix(
        widget_size,
        &Mat4::from_scale(Vec3::splat(BUTTON_HOVER_SCALE.mul_add(pos, 1.0))),
    )
}

fn set_anim_color(key_state: &KeyState, rect: &mut Rectangle, pos: f32) {
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
    _keyboard_state: Rc<RefCell<KeyboardState>>,
    data: &mut event::CallbackData,
) {
    data.animations.push(Animation::new(
        data.widget_id,
        10,
        AnimationEasing::OutBack,
        Box::new(move |data| {
            let rect = data.obj.get_as_mut::<Rectangle>();
            set_anim_color(&key_state, rect, data.pos);
            data.data.transform = get_anim_transform(data.pos, data.widget_size);
            data.needs_redraw = true;
        }),
    ));
}

fn on_leave_anim(
    key_state: Rc<KeyState>,
    _keyboard_state: Rc<RefCell<KeyboardState>>,
    data: &mut event::CallbackData,
) {
    data.animations.push(Animation::new(
        data.widget_id,
        15,
        AnimationEasing::OutQuad,
        Box::new(move |data| {
            let rect = data.obj.get_as_mut::<Rectangle>();
            set_anim_color(&key_state, rect, 1.0 - data.pos);
            data.data.transform = get_anim_transform(1.0 - data.pos, data.widget_size);
            data.needs_redraw = true;
        }),
    ));
}

fn on_press_anim(key_state: Rc<KeyState>, data: &mut event::CallbackData) {
    if key_state.drawn_state.get() {
        return;
    }
    let rect = data.obj.get_as_mut::<Rectangle>();
    rect.params.border_color = Color::new(1.0, 1.0, 1.0, 1.0);
    data.needs_redraw = true;
    key_state.drawn_state.set(true);
}

fn on_release_anim(key_state: Rc<KeyState>, data: &mut event::CallbackData) {
    if !key_state.drawn_state.get() {
        return;
    }
    let rect = data.obj.get_as_mut::<Rectangle>();
    rect.params.border_color = key_state.border_color;
    data.needs_redraw = true;
    key_state.drawn_state.set(false);
}
