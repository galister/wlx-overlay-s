use std::{cell::RefCell, collections::HashMap, rc::Rc};

use glam::{Affine2, vec2, vec3a};
use wgui::{
    animation::{Animation, AnimationEasing},
    event::{self, EventListener},
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
    KEYBOARD_NAME, KeyState, KeyboardBackend, KeyboardState, handle_press, handle_release,
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
        hid: app.hid_provider.clone(),
        audio: app.audio_provider.clone(),
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

    let size = vec2(
        layout.row_size * PIXELS_PER_UNIT,
        (layout.main_layout.len() as f32) * PIXELS_PER_UNIT,
    );

    let mut panel = GuiPanel::new_blank(app, 2048)?;

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

    let (_, mut gui_state_key) =
        wgui::parser::new_layout_from_assets(Box::new(gui::asset::GuiAsset {}), "keyboard.xml")?;

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
                        .widget_states
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
                    })
                };

                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseEnter(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data| {
                            on_enter_anim(k.clone(), kb.clone(), data);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseLeave(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data| {
                            on_leave_anim(k.clone(), kb.clone(), data);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MousePress(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, button| {
                            on_press_anim(k.clone(), data);
                            handle_press(k.clone(), kb.clone(), button);
                        }
                    })),
                );
                panel.layout.add_event_listener(
                    *widget_id,
                    EventListener::MouseRelease(Box::new({
                        let (k, kb) = (key_state.clone(), state.clone());
                        move |data, button| {
                            on_release_anim(k.clone(), data);
                            handle_release(k.clone(), kb.clone(), button);
                        }
                    })),
                );
            }
        }
    }

    let interaction_transform = Affine2::from_translation(vec2(0.5, 0.5))
        * Affine2::from_scale(vec2(1., -size.x as f32 / size.y as f32));

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

fn on_enter_anim(
    key_state: Rc<KeyState>,
    _keyboard_state: Rc<RefCell<KeyboardState>>,
    data: &mut event::CallbackData,
) {
    data.animations.push(Animation::new(
        data.widget_id,
        5,
        AnimationEasing::OutQuad,
        Box::new(move |data| {
            let rect = data.obj.get_as_mut::<Rectangle>();
            let brightness = data.pos * 0.5;
            rect.params.color.r = key_state.color.r + brightness;
            rect.params.color.g = key_state.color.g + brightness;
            rect.params.color.b = key_state.color.b + brightness;
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
        5,
        AnimationEasing::OutQuad,
        Box::new(move |data| {
            let rect = data.obj.get_as_mut::<Rectangle>();
            let brightness = (1.0 - data.pos) * 0.5;
            rect.params.color.r = key_state.color.r + brightness;
            rect.params.color.g = key_state.color.g + brightness;
            rect.params.color.b = key_state.color.b + brightness;
            data.needs_redraw = true;
        }),
    ));
}

fn on_press_anim(key_state: Rc<KeyState>, data: &mut event::CallbackData) {
    let rect = data.obj.get_as_mut::<Rectangle>();
    rect.params.border_color = key_state.border_color;
    data.needs_redraw = true;
}

fn on_release_anim(key_state: Rc<KeyState>, data: &mut event::CallbackData) {
    let rect = data.obj.get_as_mut::<Rectangle>();
    rect.params.border_color = key_state.border_color;
    data.needs_redraw = true;
}
