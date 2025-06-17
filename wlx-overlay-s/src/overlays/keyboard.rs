use std::{
    collections::HashMap,
    process::Child,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use crate::{
    backend::{
        input::InteractionHandler,
        overlay::{
            FrameMeta, OverlayBackend, OverlayData, OverlayRenderer, OverlayState, Positioning,
            ShouldRender,
        },
    },
    config::{self, ConfigType},
    graphics::CommandBuffers,
    gui::panel::GuiPanel,
    hid::{
        ALT, CTRL, KEYS_TO_MODS, KeyModifier, KeyType, META, NUM_LOCK, SHIFT, SUPER, VirtualKey,
        XkbKeymap, get_key_type,
    },
    state::{AppState, KeyboardFocus},
};
use glam::{Affine2, vec2, vec3a};
use regex::Regex;
use serde::{Deserialize, Serialize};
use vulkano::image::view::ImageView;
use wgui::{
    parser::parse_color_hex,
    taffy::{self, prelude::length},
    widget::{
        div::Div,
        rectangle::{Rectangle, RectangleParams},
        util::WLength,
    },
};

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
                wayvr.borrow_mut().data.state.send_key(key as u32, down);
            }
        }
    }
}

fn set_modifiers(app: &mut AppState, mods: u8) {
    match app.keyboard_focus {
        KeyboardFocus::PhysicalScreen => {
            app.hid_provider.set_modifiers(mods);
        }
        KeyboardFocus::WayVR =>
        {
            #[cfg(feature = "wayvr")]
            if let Some(wayvr) = &app.wayvr {
                wayvr.borrow_mut().data.state.set_modifiers(mods);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
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

    let padding = 4f32;

    let mut panel = GuiPanel::new_blank(
        app,
        padding.mul_add(2.0, size.x) as u32,
        padding.mul_add(2.0, size.y) as u32,
    )?;

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
            padding: length(padding),
            ..Default::default()
        },
    )?;

    let has_altgr = keymap
        .as_ref()
        .is_some_and(super::super::hid::XkbKeymap::has_altgr);

    if !LAYOUT.auto_labels.unwrap_or(true) {
        keymap = None;
    }

    for row in 0..LAYOUT.key_sizes.len() {
        let (div, _) = panel.layout.add_child(
            background,
            Div::create().unwrap(),
            taffy::Style {
                flex_direction: taffy::FlexDirection::Row,
                ..Default::default()
            },
        )?;

        for col in 0..LAYOUT.key_sizes[row].len() {
            let my_size = LAYOUT.key_sizes[row][col];
            let my_size = taffy::Size {
                width: length(PIXELS_PER_UNIT * my_size),
                height: length(PIXELS_PER_UNIT),
            };

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

                                if label0.chars().next().is_some_and(char::is_alphabetic) {
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
                        log::error!("Keyboard: EXEC args empty for {key}");
                    } else {
                        let mut iter = exec_args.iter().cloned();
                        if let Some(program) = iter.next() {
                            maybe_state = Some(KeyButtonData::Exec {
                                program,
                                args: iter.by_ref().take_while(|arg| arg[..] != *"null").collect(),
                                release_program: iter.next(),
                                release_args: iter.collect(),
                            });
                        }
                    }
                } else {
                    log::error!("Unknown key: {key}");
                }

                if let Some(state) = maybe_state {
                    if label.is_empty() {
                        label = LAYOUT.label_for_key(key);
                    }
                    let _ = panel.layout.add_child(
                        div,
                        Rectangle::create(RectangleParams {
                            border_color: parse_color_hex("#dddddd").unwrap(),
                            border: 2.0,
                            round: WLength::Units(4.0),
                            ..Default::default()
                        })
                        .unwrap(),
                        taffy::Style {
                            size: my_size,
                            min_size: my_size,
                            max_size: my_size,
                            ..Default::default()
                        },
                    )?;
                } else {
                    let _ = panel.layout.add_child(
                        div,
                        Div::create().unwrap(),
                        taffy::Style {
                            size: my_size,
                            min_size: my_size,
                            max_size: my_size,
                            ..Default::default()
                        },
                    )?;
                }
            }
        }
    }

    let interaction_transform = Affine2::from_translation(vec2(0.5, 0.5))
        * Affine2::from_scale(vec2(1., -size.x as f32 / size.y as f32));

    let width = LAYOUT.row_size * 0.05 * app.session.config.keyboard_scale;

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
        backend: Box::new(KeyboardBackend { panel }),
        ..Default::default()
    })
}

struct KeyboardData {
    modifiers: KeyModifier,
    alt_modifier: KeyModifier,
    processes: Vec<Child>,
}

const KEY_AUDIO_WAV: &[u8] = include_bytes!("../res/421581.wav");

fn key_click(app: &mut AppState) {
    if app.session.config.keyboard_sound_enabled {
        app.audio.play(KEY_AUDIO_WAV);
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

static LAYOUT: LazyLock<Layout> = LazyLock::new(Layout::load_from_disk);

static MACRO_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_-]+)(?: +(UP|DOWN))?$").unwrap()); // want panic

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize)]
#[repr(usize)]
pub enum AltModifier {
    #[default]
    None,
    Shift,
    Ctrl,
    Alt,
    Super,
    Meta,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(clippy::struct_field_names)]
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
    fn load_from_disk() -> Self {
        let mut layout = config::load_known_yaml::<Self>(ConfigType::Keyboard);
        layout.post_load();
        layout
    }

    fn post_load(&mut self) {
        for i in 0..self.key_sizes.len() {
            let row = &self.key_sizes[i];
            let width: f32 = row.iter().sum();
            assert!(
                (width - self.row_size).abs() < 0.001,
                "Row {} has a width of {}, but the row size is {}",
                i,
                width,
                self.row_size
            );
        }

        for i in 0..self.main_layout.len() {
            let row = &self.main_layout[i];
            let width = row.len();
            assert!(
                (width == self.key_sizes[i].len()),
                "Row {} has {} keys, needs to have {} according to key_sizes",
                i,
                width,
                self.key_sizes[i].len()
            );
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
                log::error!("keyboard.yaml: Key '{key}' must not start or end with '_'!");
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
    panel: GuiPanel,
}

impl OverlayBackend for KeyboardBackend {
    fn set_interaction(&mut self, interaction: Box<dyn crate::backend::input::InteractionHandler>) {
        self.panel.set_interaction(interaction);
    }
    fn set_renderer(&mut self, renderer: Box<dyn crate::backend::overlay::OverlayRenderer>) {
        self.panel.set_renderer(renderer);
    }
}

impl InteractionHandler for KeyboardBackend {
    fn on_pointer(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
        pressed: bool,
    ) {
        self.panel.on_pointer(app, hit, pressed);
    }
    fn on_scroll(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
        delta_y: f32,
        delta_x: f32,
    ) {
        self.panel.on_scroll(app, hit, delta_y, delta_x);
    }
    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.panel.on_left(app, pointer);
    }
    fn on_hover(
        &mut self,
        app: &mut AppState,
        hit: &crate::backend::input::PointerHit,
    ) -> Option<crate::backend::input::Haptics> {
        self.panel.on_hover(app, hit)
    }
}

impl OverlayRenderer for KeyboardBackend {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.init(app)
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        self.panel.should_render(app)
    }
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        self.panel.render(app, tgt, buf, alpha)
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.panel.frame_meta()
    }
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        set_modifiers(app, 0);
        self.panel.pause(app)
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.resume(app)
    }
}

pub enum KeyCapType {
    /// Label is in center of keycap
    Regular,
    /// Label on the top
    /// AltGr symbol on bottom
    RegularAltGr,
    /// Primary symbol on bottom
    /// Shift symbol on top
    Reversed,
    /// Primary symbol on bottom-left
    /// Shift symbol on top-left
    /// AltGr symbol on bottom-right
    ReversedAltGr,
}
