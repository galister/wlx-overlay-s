use std::{
    collections::HashMap,
    process::{Child, Command},
    str::FromStr,
};

use crate::{
    backend::{
        input::{InteractionHandler, PointerMode},
        overlay::{FrameTransform, OverlayBackend, OverlayData, OverlayRenderer, OverlayState},
    },
    config::{self, ConfigType},
    gui::{
        canvas::{builder::CanvasBuilder, control::Control, Canvas},
        color_parse, KeyCapType,
    },
    hid::{
        get_key_type, KeyModifier, KeyType, VirtualKey, XkbKeymap, ALT, CTRL, KEYS_TO_MODS, META,
        NUM_LOCK, SHIFT, SUPER,
    },
    state::{AppState, KeyboardFocus},
};
use glam::{vec2, vec3a, Affine2, Vec4};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

const PIXELS_PER_UNIT: f32 = 80.;
const BUTTON_PADDING: f32 = 4.;
const AUTO_RELEASE_MODS: [KeyModifier; 5] = [SHIFT, CTRL, ALT, SUPER, META];

pub const KEYBOARD_NAME: &str = "kbd";

fn send_key(app: &mut AppState, key: VirtualKey, down: bool) {
    match app.keyboard_focus {
        KeyboardFocus::PhysicalScreen => {
            app.hid_provider.send_key(key, down);
        }
        KeyboardFocus::WayVR =>
        {
            #[cfg(feature = "wayvr")]
            if let Some(wayvr) = &app.wayvr {
                wayvr.borrow_mut().state.send_key(key as u32, down);
            }
        }
    }
}

fn set_modifiers(app: &mut AppState, mods: u8) {
    match app.keyboard_focus {
        KeyboardFocus::PhysicalScreen => {
            app.hid_provider.set_modifiers(mods);
        }
        KeyboardFocus::WayVR => {}
    }
}

pub fn create_keyboard<O>(
    app: &AppState,
    mut keymap: Option<XkbKeymap>,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let size = vec2(
        LAYOUT.row_size * PIXELS_PER_UNIT,
        (LAYOUT.main_layout.len() as f32) * PIXELS_PER_UNIT,
    );

    let data = KeyboardData {
        modifiers: 0,
        alt_modifier: match LAYOUT.alt_modifier {
            AltModifier::Shift => SHIFT,
            AltModifier::Ctrl => CTRL,
            AltModifier::Alt => ALT,
            AltModifier::Super => SUPER,
            AltModifier::Meta => META,
            _ => 0,
        },
        processes: vec![],
    };

    let mut canvas = CanvasBuilder::new(
        size.x as _,
        size.y as _,
        app.graphics.clone(),
        app.graphics.native_format,
        data,
    )?;

    canvas.bg_color = color_parse("#181926").unwrap(); //safe
    canvas.panel(0., 0., size.x, size.y, 12.);

    canvas.font_size = 18;
    canvas.fg_color = color_parse("#cad3f5").unwrap(); //safe
    canvas.bg_color = color_parse("#1e2030").unwrap(); //safe

    let has_altgr = keymap.as_ref().map_or(false, |k| k.has_altgr());

    if !LAYOUT.auto_labels.unwrap_or(true) {
        keymap = None;
    }

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
                let mut label = Vec::with_capacity(2);
                let mut maybe_state: Option<KeyButtonData> = None;
                let mut cap_type = KeyCapType::Regular;

                if let Ok(vk) = VirtualKey::from_str(key) {
                    if let Some(keymap) = keymap.as_ref() {
                        match get_key_type(vk) {
                            KeyType::Symbol => {
                                let label0 = keymap.label_for_key(vk, 0);
                                let label1 = keymap.label_for_key(vk, SHIFT);

                                if label0.chars().next().map_or(false, |f| f.is_alphabetic()) {
                                    label.push(label1);
                                    if has_altgr {
                                        cap_type = KeyCapType::RegularAltGr;
                                        label.push(keymap.label_for_key(vk, META));
                                    } else {
                                        cap_type = KeyCapType::Regular;
                                    }
                                } else {
                                    label.push(label0);
                                    label.push(label1);
                                    if has_altgr {
                                        label.push(keymap.label_for_key(vk, META));
                                        cap_type = KeyCapType::ReversedAltGr;
                                    } else {
                                        cap_type = KeyCapType::Reversed;
                                    }
                                }
                            }
                            KeyType::NumPad => {
                                label.push(keymap.label_for_key(vk, NUM_LOCK));
                            }
                            KeyType::Other => {}
                        }
                    }

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
                    let mut iter = exec_args.iter().cloned();
                    if let Some(program) = iter.next() {
                        maybe_state = Some(KeyButtonData::Exec {
                            program,
                            args: iter.by_ref().take_while(|arg| arg[..] != *"null").collect(),
                            release_program: iter.next(),
                            release_args: iter.collect(),
                        })
                    };
                } else {
                    log::error!("Unknown key: {}", key);
                }

                if let Some(state) = maybe_state {
                    if label.is_empty() {
                        label = LAYOUT.label_for_key(key);
                    }
                    let button = canvas.key_button(x, y, w, h, 12., cap_type, &label);
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
            anchored: true,
            interactable: true,
            spawn_scale: width,
            spawn_point: vec3a(0., -0.5, 0.),
            interaction_transform,
            ..Default::default()
        },
        backend: Box::new(KeyboardBackend { canvas }),
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

            data.modifiers |= match mode {
                PointerMode::Right => SHIFT,
                PointerMode::Middle => data.alt_modifier,
                _ => 0,
            };

            app.hid_provider.set_modifiers(data.modifiers);

            send_key(app, *vk, true);
            *pressed = true;
        }
        Some(KeyButtonData::Modifier { modifier, sticky }) => {
            *sticky = data.modifiers & *modifier == 0;
            data.modifiers |= *modifier;
            data.key_click(app);
            set_modifiers(app, data.modifiers);
        }
        Some(KeyButtonData::Macro { verbs }) => {
            data.key_click(app);
            for (vk, press) in verbs {
                send_key(app, *vk, *press);
            }
        }
        Some(KeyButtonData::Exec { program, args, .. }) => {
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
            send_key(app, *vk, false);
            *pressed = false;

            for m in AUTO_RELEASE_MODS.iter() {
                if data.modifiers & *m != 0 {
                    data.modifiers &= !*m;
                    set_modifiers(app, data.modifiers);
                }
            }
        }
        Some(KeyButtonData::Modifier { modifier, sticky }) => {
            if !*sticky {
                data.modifiers &= !*modifier;
                set_modifiers(app, data.modifiers);
            }
        }
        Some(KeyButtonData::Exec {
            release_program,
            release_args,
            ..
        }) => {
            // Reap previous processes
            data.processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            if let Some(program) = release_program {
                if let Ok(child) = Command::new(program).args(release_args).spawn() {
                    data.processes.push(child);
                }
            }
        }
        _ => {}
    }
}

static PRESS_COLOR: Vec4 = Vec4::new(198. / 255., 160. / 255., 246. / 255., 0.5);

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
    alt_modifier: KeyModifier,
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
    Key {
        vk: VirtualKey,
        pressed: bool,
    },
    Modifier {
        modifier: KeyModifier,
        sticky: bool,
    },
    Macro {
        verbs: Vec<(VirtualKey, bool)>,
    },
    Exec {
        program: String,
        args: Vec<String>,
        release_program: Option<String>,
        release_args: Vec<String>,
    },
}

static LAYOUT: Lazy<Layout> = Lazy::new(Layout::load_from_disk);

static MACRO_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-9_-]+)(?: +(UP|DOWN))?$").unwrap()); // want panic

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[repr(usize)]
pub enum AltModifier {
    None,
    Shift,
    Ctrl,
    Alt,
    Super,
    Meta,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Layout {
    name: String,
    row_size: f32,
    key_sizes: Vec<Vec<f32>>,
    main_layout: Vec<Vec<Option<String>>>,
    alt_modifier: AltModifier,
    exec_commands: HashMap<String, Vec<String>>,
    macros: HashMap<String, Vec<String>>,
    labels: HashMap<String, Vec<String>>,
    auto_labels: Option<bool>,
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

struct KeyboardBackend {
    canvas: Canvas<KeyboardData, KeyButtonData>,
}

impl OverlayBackend for KeyboardBackend {
    fn set_interaction(&mut self, interaction: Box<dyn crate::backend::input::InteractionHandler>) {
        self.canvas.set_interaction(interaction)
    }
    fn set_renderer(&mut self, renderer: Box<dyn crate::backend::overlay::OverlayRenderer>) {
        self.canvas.set_renderer(renderer)
    }
}

impl InteractionHandler for KeyboardBackend {
    fn on_pointer(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
        pressed: bool,
    ) {
        self.canvas.on_pointer(app, hit, pressed)
    }
    fn on_scroll(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
        delta: f32,
    ) {
        self.canvas.on_scroll(app, hit, delta)
    }
    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.canvas.on_left(app, pointer)
    }
    fn on_hover(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
    ) -> Option<crate::backend::input::Haptics> {
        self.canvas.on_hover(app, hit)
    }
}

impl OverlayRenderer for KeyboardBackend {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.canvas.init(app)
    }
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.canvas.render(app)
    }
    fn frame_transform(&mut self) -> Option<FrameTransform> {
        self.canvas.frame_transform()
    }
    fn view(&mut self) -> Option<std::sync::Arc<vulkano::image::view::ImageView>> {
        self.canvas.view()
    }
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.canvas.data_mut().modifiers = 0;
        set_modifiers(app, 0);
        self.canvas.pause(app)
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.canvas.resume(app)
    }
}
