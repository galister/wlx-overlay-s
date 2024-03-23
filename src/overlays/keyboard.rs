use std::{
    collections::HashMap,
    process::{Child, Command},
    str::FromStr,
};

use crate::{
    backend::{
        input::PointerMode,
        overlay::{OverlayData, OverlayState},
    },
    config::{self, ConfigType},
    gui::{color_parse, CanvasBuilder, Control},
    hid::{KeyModifier, VirtualKey, ALT, CTRL, KEYS_TO_MODS, META, SHIFT, SUPER},
    state::AppState,
};
use glam::{vec2, vec3a, Affine2, Vec4};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

const PIXELS_PER_UNIT: f32 = 80.;
const BUTTON_PADDING: f32 = 4.;
const AUTO_RELEASE_MODS: [KeyModifier; 5] = [SHIFT, CTRL, ALT, SUPER, META];

pub const KEYBOARD_NAME: &str = "kbd";

pub fn create_keyboard<O>(app: &AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let size = vec2(
        LAYOUT.row_size * PIXELS_PER_UNIT,
        (LAYOUT.main_layout.len() as f32) * PIXELS_PER_UNIT,
    );

    let data = KeyboardData {
        modifiers: 0,
        processes: vec![],
    };

    let mut canvas = CanvasBuilder::new(
        size.x as _,
        size.y as _,
        app.graphics.clone(),
        app.format,
        data,
    )?;

    canvas.bg_color = color_parse("#101010").unwrap(); //safe
    canvas.panel(0., 0., size.x, size.y);

    canvas.font_size = 18;
    canvas.bg_color = color_parse("#202020").unwrap(); //safe

    let unit_size = size.x / LAYOUT.row_size;
    let h = unit_size - 2. * BUTTON_PADDING;

    for row in 0..LAYOUT.key_sizes.len() {
        let y = unit_size * (row as f32) + BUTTON_PADDING;
        let mut sum_size = 0f32;

        for col in 0..LAYOUT.key_sizes[row].len() {
            let my_size = LAYOUT.key_sizes[row][col];
            let x = unit_size * sum_size + BUTTON_PADDING;
            let w = unit_size * my_size - 2. * BUTTON_PADDING;

            if let Some(key) = LAYOUT.main_layout[row][col].as_ref() {
                let mut maybe_state: Option<KeyButtonData> = None;
                if let Ok(vk) = VirtualKey::from_str(key) {
                    if let Some(mods) = KEYS_TO_MODS.get(vk) {
                        maybe_state = Some(KeyButtonData::Modifier {
                            modifier: *mods,
                            sticky: false,
                        });
                    } else {
                        maybe_state = Some(KeyButtonData::Key { vk, pressed: false });
                    }
                } else if let Some(macro_verbs) = LAYOUT.macros.get(key) {
                    maybe_state = Some(KeyButtonData::Macro {
                        verbs: key_events_for_macro(macro_verbs),
                    });
                } else if let Some(exec_args) = LAYOUT.exec_commands.get(key) {
                    if exec_args.is_empty() {
                        log::error!("Keyboard: EXEC args empty for {}", key);
                        continue;
                    }
                    maybe_state = Some(KeyButtonData::Exec {
                        program: exec_args
                            .first()
                            .unwrap() // safe because we checked is_empty
                            .clone(),
                        args: exec_args.iter().skip(1).cloned().collect(),
                    });
                } else {
                    log::error!("Unknown key: {}", key);
                }

                if let Some(state) = maybe_state {
                    let label = LAYOUT.label_for_key(key);
                    let button = canvas.key_button(x, y, w, h, &label);
                    button.state = Some(state);
                    button.on_press = Some(key_press);
                    button.on_release = Some(key_release);
                    button.test_highlight = Some(test_highlight);
                }
            }

            sum_size += my_size;
        }
    }

    let canvas = canvas.build();

    let interaction_transform = Affine2::from_translation(vec2(0.5, 0.5))
        * Affine2::from_scale(vec2(1., -size.x as f32 / size.y as f32));

    let width = LAYOUT.row_size * 0.05 * app.session.config.keyboard_scale;

    Ok(OverlayData {
        state: OverlayState {
            name: KEYBOARD_NAME.into(),
            grabbable: true,
            recenter: true,
            interactable: true,
            spawn_scale: width,
            spawn_point: vec3a(0., -0.5, -1.),
            interaction_transform,
            ..Default::default()
        },
        backend: Box::new(canvas),
        ..Default::default()
    })
}

fn key_press(
    control: &mut Control<KeyboardData, KeyButtonData>,
    data: &mut KeyboardData,
    app: &mut AppState,
    mode: PointerMode,
) {
    match control.state.as_mut() {
        Some(KeyButtonData::Key { vk, pressed }) => {
            data.key_click(app);

            if let PointerMode::Right = mode {
                data.modifiers |= SHIFT;
                app.hid_provider.set_modifiers(data.modifiers);
            }

            app.hid_provider.send_key(*vk as _, true);
            *pressed = true;
        }
        Some(KeyButtonData::Modifier { modifier, sticky }) => {
            *sticky = data.modifiers & *modifier == 0;
            data.modifiers |= *modifier;
            data.key_click(app);
            app.hid_provider.set_modifiers(data.modifiers);
        }
        Some(KeyButtonData::Macro { verbs }) => {
            data.key_click(app);
            for (vk, press) in verbs {
                app.hid_provider.send_key(*vk as _, *press);
            }
        }
        Some(KeyButtonData::Exec { program, args }) => {
            // Reap previous processes
            data.processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            data.key_click(app);
            if let Ok(child) = Command::new(program).args(args).spawn() {
                data.processes.push(child);
            }
        }
        None => {}
    }
}

fn key_release(
    control: &mut Control<KeyboardData, KeyButtonData>,
    data: &mut KeyboardData,
    app: &mut AppState,
) {
    match control.state.as_mut() {
        Some(KeyButtonData::Key { vk, pressed }) => {
            app.hid_provider.send_key(*vk as _, false);
            *pressed = false;

            for m in AUTO_RELEASE_MODS.iter() {
                if data.modifiers & *m != 0 {
                    data.modifiers &= !*m;
                    app.hid_provider.set_modifiers(data.modifiers);
                }
            }
        }
        Some(KeyButtonData::Modifier { modifier, sticky }) => {
            if !*sticky {
                data.modifiers &= !*modifier;
                app.hid_provider.set_modifiers(data.modifiers);
            }
        }
        _ => {}
    }
}

static PRESS_COLOR: Vec4 = Vec4::new(1.0, 1.0, 1.0, 0.5);

fn test_highlight(
    control: &Control<KeyboardData, KeyButtonData>,
    data: &mut KeyboardData,
    _app: &mut AppState,
) -> Option<Vec4> {
    let pressed = match control.state.as_ref() {
        Some(KeyButtonData::Key { pressed, .. }) => *pressed,
        Some(KeyButtonData::Modifier { modifier, .. }) => data.modifiers & *modifier != 0,
        _ => false,
    };

    if pressed {
        Some(PRESS_COLOR)
    } else {
        None
    }
}

struct KeyboardData {
    modifiers: KeyModifier,
    processes: Vec<Child>,
}

const KEY_AUDIO_WAV: &[u8] = include_bytes!("../res/421581.wav");

impl KeyboardData {
    fn key_click(&mut self, app: &mut AppState) {
        if app.session.config.keyboard_sound_enabled {
            app.audio.play(KEY_AUDIO_WAV);
        }
    }
}

enum KeyButtonData {
    Key { vk: VirtualKey, pressed: bool },
    Modifier { modifier: KeyModifier, sticky: bool },
    Macro { verbs: Vec<(VirtualKey, bool)> },
    Exec { program: String, args: Vec<String> },
}

static LAYOUT: Lazy<Layout> = Lazy::new(Layout::load_from_disk);

static MACRO_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-1_-]+)(?: +(UP|DOWN))?$").unwrap()); // want panic

#[derive(Debug, Deserialize, Serialize)]
pub struct Layout {
    name: String,
    row_size: f32,
    key_sizes: Vec<Vec<f32>>,
    main_layout: Vec<Vec<Option<String>>>,
    exec_commands: HashMap<String, Vec<String>>,
    macros: HashMap<String, Vec<String>>,
    labels: HashMap<String, Vec<String>>,
}

impl Layout {
    fn load_from_disk() -> Layout {
        let mut layout = config::load_known_yaml::<Layout>(ConfigType::Keyboard);
        layout.post_load();
        layout
    }

    fn post_load(&mut self) {
        for i in 0..self.key_sizes.len() {
            let row = &self.key_sizes[i];
            let width: f32 = row.iter().sum();
            if (width - self.row_size).abs() > 0.001 {
                panic!(
                    "Row {} has a width of {}, but the row size is {}",
                    i, width, self.row_size
                );
            }
        }

        for i in 0..self.main_layout.len() {
            let row = &self.main_layout[i];
            let width = row.len();
            if width != self.key_sizes[i].len() {
                panic!(
                    "Row {} has {} keys, needs to have {} according to key_sizes",
                    i,
                    width,
                    self.key_sizes[i].len()
                );
            }
        }
    }

    fn label_for_key(&self, key: &str) -> Vec<String> {
        if let Some(label) = self.labels.get(key) {
            return label.clone();
        }
        if key.is_empty() {
            return vec![];
        }
        if key.len() == 1 {
            return vec![key.to_string().to_lowercase()];
        }
        let mut key = key;
        if key.starts_with("KP_") {
            key = &key[3..];
        }
        if key.contains('_') {
            key = key.split('_').next().unwrap_or_else(|| {
                log::error!(
                    "keyboard.yaml: Key '{}' must not start or end with '_'!",
                    key
                );
                "???"
            });
        }
        vec![format!(
            "{}{}",
            key.chars().next().unwrap().to_uppercase(), // safe because we checked is_empty
            &key[1..].to_lowercase()
        )]
    }
}

fn key_events_for_macro(macro_verbs: &Vec<String>) -> Vec<(VirtualKey, bool)> {
    let mut key_events = vec![];
    for verb in macro_verbs {
        if let Some(caps) = MACRO_REGEX.captures(verb) {
            if let Ok(virtual_key) = VirtualKey::from_str(&caps[1]) {
                if let Some(state) = caps.get(2) {
                    if state.as_str() == "UP" {
                        key_events.push((virtual_key, false));
                    } else if state.as_str() == "DOWN" {
                        key_events.push((virtual_key, true));
                    } else {
                        log::error!(
                            "Unknown key state in macro: {}, looking for UP or DOWN.",
                            state.as_str()
                        );
                        return vec![];
                    }
                } else {
                    key_events.push((virtual_key, true));
                    key_events.push((virtual_key, false));
                }
            } else {
                log::error!("Unknown virtual key: {}", &caps[1]);
                return vec![];
            }
        }
    }
    key_events
}
