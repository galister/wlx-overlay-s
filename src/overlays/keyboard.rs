use std::{
    collections::HashMap,
    env::var,
    fs,
    io::Cursor,
    path::PathBuf,
    process::{Child, Command},
    str::FromStr,
    sync::Arc,
};

use crate::{
    backend::overlay::{OverlayData, OverlayState},
    gui::{color_parse, CanvasBuilder, Control},
    hid::{KeyModifier, VirtualKey, KEYS_TO_MODS},
    state::AppState,
};
use glam::{vec2, vec3a};
use once_cell::sync::Lazy;
use regex::Regex;
use rodio::{Decoder, OutputStream, Source};
use serde::{Deserialize, Serialize};

const PIXELS_PER_UNIT: f32 = 80.;
const BUTTON_PADDING: f32 = 4.;

pub fn create_keyboard<O>(app: &AppState) -> OverlayData<O>
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
        audio_stream: None,
    };

    let mut canvas = CanvasBuilder::new(
        size.x as _,
        size.y as _,
        app.graphics.clone(),
        app.format,
        data,
    );

    canvas.bg_color = color_parse("#101010");
    canvas.panel(0., 0., size.x, size.y);

    canvas.font_size = 18;
    canvas.bg_color = color_parse("#202020");

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
                            pressed: false,
                        });
                    } else {
                        maybe_state = Some(KeyButtonData::Key { vk, pressed: false });
                    }
                } else if let Some(macro_verbs) = LAYOUT.macros.get(key) {
                    maybe_state = Some(KeyButtonData::Macro {
                        verbs: key_events_for_macro(macro_verbs),
                    });
                } else if let Some(exec_args) = LAYOUT.exec_commands.get(key) {
                    maybe_state = Some(KeyButtonData::Exec {
                        program: exec_args.first().unwrap().clone(),
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

    OverlayData {
        state: OverlayState {
            name: Arc::from("kbd"),
            show_hide: true,
            width: LAYOUT.row_size * 0.05,
            size: (size.x as _, size.y as _),
            grabbable: true,
            spawn_point: vec3a(0., -0.5, -1.),
            ..Default::default()
        },
        backend: Box::new(canvas),
        ..Default::default()
    }
}

fn key_press(
    control: &mut Control<KeyboardData, KeyButtonData>,
    data: &mut KeyboardData,
    app: &mut AppState,
) {
    match control.state.as_mut() {
        Some(KeyButtonData::Key { vk, pressed }) => {
            data.key_click();
            app.hid_provider.send_key(*vk as _, true);
            *pressed = true;
        }
        Some(KeyButtonData::Modifier {
            modifier,
            sticky,
            pressed,
        }) => {
            *sticky = data.modifiers & *modifier == 0;
            data.modifiers |= *modifier;
            data.key_click();
            app.hid_provider.set_modifiers(data.modifiers);
            *pressed = true;
        }
        Some(KeyButtonData::Macro { verbs }) => {
            data.key_click();
            for (vk, press) in verbs {
                app.hid_provider.send_key(*vk as _, *press);
            }
        }
        Some(KeyButtonData::Exec { program, args }) => {
            // Reap previous processes
            data.processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            data.key_click();
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
        }
        Some(KeyButtonData::Modifier {
            modifier,
            sticky,
            pressed,
        }) => {
            if !*sticky {
                data.modifiers &= !*modifier;
                app.hid_provider.set_modifiers(data.modifiers);
                *pressed = false;
            }
        }
        _ => {}
    }
}

fn test_highlight(
    control: &Control<KeyboardData, KeyButtonData>,
    _data: &mut KeyboardData,
    _app: &mut AppState,
) -> bool {
    match control.state.as_ref() {
        Some(KeyButtonData::Key { pressed, .. }) => *pressed,
        Some(KeyButtonData::Modifier { pressed, .. }) => *pressed,
        _ => false,
    }
}

struct KeyboardData {
    modifiers: KeyModifier,
    processes: Vec<Child>,
    audio_stream: Option<OutputStream>,
}

impl KeyboardData {
    fn key_click(&mut self) {
        let wav = include_bytes!("../res/421581.wav");
        let cursor = Cursor::new(wav);
        let source = Decoder::new_wav(cursor).unwrap();
        self.audio_stream = None;
        if let Ok((stream, handle)) = OutputStream::try_default() {
            let _ = handle.play_raw(source.convert_samples());
            self.audio_stream = Some(stream);
        } else {
            log::error!("Failed to play key click");
        }
    }
}

enum KeyButtonData {
    Key {
        vk: VirtualKey,
        pressed: bool,
    },
    Modifier {
        modifier: KeyModifier,
        sticky: bool,
        pressed: bool,
    },
    Macro {
        verbs: Vec<(VirtualKey, bool)>,
    },
    Exec {
        program: String,
        args: Vec<String>,
    },
}

static KEYBOARD_YAML: Lazy<PathBuf> = Lazy::new(|| {
    let home = &var("HOME").unwrap();
    [home, ".config/wlxoverlay/keyboard.yaml"].iter().collect() //TODO other paths
});

static LAYOUT: Lazy<Layout> = Lazy::new(Layout::load_from_disk);

static MACRO_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-1_-]+)(?: +(UP|DOWN))?$").unwrap());

#[derive(Debug, Deserialize, Serialize)]
struct Layout {
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
        let mut yaml = fs::read_to_string(KEYBOARD_YAML.as_path()).ok();

        if yaml.is_none() {
            yaml = Some(include_str!("../res/keyboard.yaml").to_string());
        }

        let mut layout: Layout =
            serde_yaml::from_str(&yaml.unwrap()).expect("Failed to parse keyboard.yaml");
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
            key = key.split('_').next().unwrap();
        }
        vec![format!(
            "{}{}",
            key.chars().next().unwrap().to_uppercase(),
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
