use std::{
    cell::{Cell, RefCell},
    process::{Child, Command},
    rc::Rc,
    sync::Arc,
};

use vulkano::image::view::ImageView;
use wgui::{drawing, event::MouseButton};

use crate::{
    backend::{
        input::InteractionHandler,
        overlay::{FrameMeta, OverlayBackend, OverlayRenderer, ShouldRender},
    },
    graphics::CommandBuffers,
    gui::panel::GuiPanel,
    state::AppState,
    subsystem::{
        audio::{AudioOutput, AudioRole},
        hid::{ALT, CTRL, KeyModifier, META, SHIFT, SUPER, VirtualKey},
        input::HidWrapper,
    },
};

pub mod builder;
mod layout;

pub const KEYBOARD_NAME: &str = "kbd";
const AUTO_RELEASE_MODS: [KeyModifier; 5] = [SHIFT, CTRL, ALT, SUPER, META];

struct KeyboardBackend {
    panel: GuiPanel,
    state: Rc<RefCell<KeyboardState>>,
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
        self.state.borrow_mut().modifiers = 0;
        app.hid_provider.borrow_mut().set_modifiers_routed(0);
        self.panel.pause(app)
    }
    fn resume(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.panel.resume(app)
    }
}

struct KeyboardState {
    hid: Rc<RefCell<HidWrapper>>,
    audio: Rc<RefCell<AudioOutput>>,
    modifiers: KeyModifier,
    alt_modifier: KeyModifier,
    processes: Vec<Child>,
}

const KEY_AUDIO_WAV: &[u8] = include_bytes!("../../res/421581.wav");

struct KeyState {
    button_state: KeyButtonData,
    color: drawing::Color,
    color2: drawing::Color,
    border_color: drawing::Color,
}

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

fn play_key_click(keyboard: &KeyboardState) {
    keyboard
        .audio
        .borrow_mut()
        .play(AudioRole::Keyboard, KEY_AUDIO_WAV);
}

fn handle_press(key: Rc<KeyState>, keyboard: Rc<RefCell<KeyboardState>>, button: MouseButton) {
    let mut keyboard = keyboard.borrow_mut();
    match &key.button_state {
        KeyButtonData::Key { vk, pressed } => {
            keyboard.modifiers |= match button {
                MouseButton::Right => SHIFT,
                MouseButton::Middle => keyboard.alt_modifier,
                _ => 0,
            };

            {
                let mut hid = keyboard.hid.borrow_mut();
                hid.set_modifiers_routed(keyboard.modifiers);
                hid.send_key_routed(*vk, true);
            }
            pressed.set(true);
            play_key_click(&keyboard);
        }
        KeyButtonData::Modifier { modifier, sticky } => {
            sticky.set(keyboard.modifiers & *modifier == 0);
            keyboard.modifiers |= *modifier;
            keyboard
                .hid
                .borrow_mut()
                .set_modifiers_routed(keyboard.modifiers);
            play_key_click(&keyboard);
        }
        KeyButtonData::Macro { verbs } => {
            let hid = keyboard.hid.borrow_mut();
            for (vk, press) in verbs {
                hid.send_key_routed(*vk, *press);
            }
            play_key_click(&keyboard);
        }
        KeyButtonData::Exec { program, args, .. } => {
            // Reap previous processes
            keyboard
                .processes
                .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

            if let Ok(child) = Command::new(program).args(args).spawn() {
                keyboard.processes.push(child);
            }
            play_key_click(&keyboard);
        }
    }
}

fn handle_release(key: Rc<KeyState>, keyboard: Rc<RefCell<KeyboardState>>, _button: MouseButton) {
    let mut keyboard = keyboard.borrow_mut();
    match &key.button_state {
        KeyButtonData::Key { vk, pressed } => {
            pressed.set(false);

            for m in &AUTO_RELEASE_MODS {
                if keyboard.modifiers & *m != 0 {
                    keyboard.modifiers &= !*m;
                }
            }
            let mut hid = keyboard.hid.borrow_mut();
            hid.send_key_routed(*vk, false);
            hid.set_modifiers_routed(keyboard.modifiers);
        }
        KeyButtonData::Modifier { modifier, sticky } => {
            if !sticky.get() {
                keyboard.modifiers &= !*modifier;
                keyboard
                    .hid
                    .borrow_mut()
                    .set_modifiers_routed(keyboard.modifiers);
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

            if let Some(program) = release_program {
                if let Ok(child) = Command::new(program).args(release_args).spawn() {
                    keyboard.processes.push(child);
                }
            }
        }
        _ => {}
    }
}
