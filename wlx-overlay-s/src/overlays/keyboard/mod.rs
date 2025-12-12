use std::{
    cell::Cell,
    process::{Child, Command},
};

use wgui::{
    drawing,
    event::{InternalStateChangeEvent, MouseButton, MouseButtonIndex},
};

use crate::{
    backend::input::{HoverResult, PointerHit},
    gui::panel::GuiPanel,
    state::AppState,
    subsystem::hid::{ALT, CTRL, KeyModifier, META, SHIFT, SUPER, VirtualKey, WheelDelta},
    windowing::backend::{
        FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender,
    },
};

pub mod builder;
mod layout;

pub const KEYBOARD_NAME: &str = "kbd";
const AUTO_RELEASE_MODS: [KeyModifier; 5] = [SHIFT, CTRL, ALT, SUPER, META];

struct KeyboardBackend {
    panel: GuiPanel<KeyboardState>,
}

impl OverlayBackend for KeyboardBackend {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.init(app)
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        self.panel.should_render(app)
    }
    fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()> {
        self.panel.render(app, rdr)
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.panel.frame_meta()
    }
    fn pause(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.state.modifiers = 0;
        app.hid_provider.set_modifiers_routed(0);
        self.panel.pause(app)
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.resume(app)?;
        self.panel.push_event(
            app,
            &wgui::event::Event::InternalStateChange(InternalStateChangeEvent { metadata: 0 }),
        );
        Ok(())
    }

    fn notify(&mut self, app: &mut AppState, event_data: OverlayEventData) -> anyhow::Result<()> {
        self.panel.notify(app, event_data)
    }

    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        self.panel.on_pointer(app, hit, pressed);
        self.panel.push_event(
            app,
            &wgui::event::Event::InternalStateChange(InternalStateChangeEvent { metadata: 0 }),
        );
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: WheelDelta) {
        self.panel.on_scroll(app, hit, delta);
    }
    fn on_left(&mut self, app: &mut AppState, pointer: usize) {
        self.panel.on_left(app, pointer);
    }
    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> HoverResult {
        self.panel.on_hover(app, hit)
    }
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        self.panel.get_interaction_transform()
    }
}

struct KeyboardState {
    modifiers: KeyModifier,
    alt_modifier: KeyModifier,
    processes: Vec<Child>,
}

const KEY_AUDIO_WAV: &[u8] = include_bytes!("../../res/421581.wav");

fn play_key_click(app: &mut AppState) {
    app.audio_provider.play(KEY_AUDIO_WAV);
}

struct KeyState {
    button_state: KeyButtonData,
    color: drawing::Color,
    color2: drawing::Color,
    border_color: drawing::Color,
    border: f32,
    drawn_state: Cell<bool>,
}

#[derive(Debug)]
enum KeyButtonData {
    Key {
        vk: VirtualKey,
        pressed: Cell<bool>,
    },
    Modifier {
        modifier: KeyModifier,
        sticky: Cell<bool>,
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

fn handle_press(
    app: &mut AppState,
    key: &KeyState,
    keyboard: &mut KeyboardState,
    button: MouseButton,
) {
    match &key.button_state {
        KeyButtonData::Key { vk, pressed } => {
            keyboard.modifiers |= match button.index {
                MouseButtonIndex::Right => SHIFT,
                MouseButtonIndex::Middle => keyboard.alt_modifier,
                _ => 0,
            };

            app.hid_provider.set_modifiers_routed(keyboard.modifiers);
            app.hid_provider.send_key_routed(*vk, true);
            pressed.set(true);
            play_key_click(app);
        }
        KeyButtonData::Modifier { modifier, sticky } => {
            sticky.set(keyboard.modifiers & *modifier == 0);
            keyboard.modifiers |= *modifier;
            app.hid_provider.set_modifiers_routed(keyboard.modifiers);
            play_key_click(app);
        }
        KeyButtonData::Macro { verbs } => {
            for (vk, press) in verbs {
                app.hid_provider.send_key_routed(*vk, *press);
            }
            play_key_click(app);
        }
        KeyButtonData::Exec { program, args, .. } => {
            // Reap previous processes
            keyboard
                .processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            if let Ok(child) = Command::new(program).args(args).spawn() {
                keyboard.processes.push(child);
            }
            play_key_click(app);
        }
    }
}

fn handle_release(app: &mut AppState, key: &KeyState, keyboard: &mut KeyboardState) -> bool {
    match &key.button_state {
        KeyButtonData::Key { vk, pressed } => {
            pressed.set(false);

            for m in &AUTO_RELEASE_MODS {
                if keyboard.modifiers & *m != 0 {
                    keyboard.modifiers &= !*m;
                }
            }
            app.hid_provider.send_key_routed(*vk, false);
            app.hid_provider.set_modifiers_routed(keyboard.modifiers);
            true
        }
        KeyButtonData::Modifier { modifier, sticky } => {
            if sticky.get() {
                false
            } else {
                keyboard.modifiers &= !*modifier;
                app.hid_provider.set_modifiers_routed(keyboard.modifiers);
                true
            }
        }
        KeyButtonData::Exec {
            release_program,
            release_args,
            ..
        } => {
            // Reap previous processes
            keyboard
                .processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            if let Some(program) = release_program
                && let Ok(child) = Command::new(program).args(release_args).spawn()
            {
                keyboard.processes.push(child);
            }
            true
        }
        _ => true,
    }
}
