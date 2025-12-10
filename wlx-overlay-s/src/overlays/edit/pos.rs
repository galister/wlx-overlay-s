use std::{collections::HashMap, rc::Rc};

use wgui::{
    components::button::ComponentButton, event::CallbackDataCommon, layout::WidgetID,
    parser::Fetchable, renderer_vk::text::custom_glyph::CustomGlyphData,
    widget::sprite::WidgetSprite,
};
use wlx_common::{common::LeftRight, windowing::Positioning};

use crate::{
    backend::task::ModifyOverlayTask, overlays::edit::EditModeWrapPanel, windowing::window,
};

static POS_NAMES: [&str; 6] = ["static", "anchored", "floating", "hmd", "hand_l", "hand_r"];

struct PosButtonState {
    name: &'static str,
    sprite: CustomGlyphData,
    component: Rc<ComponentButton>,
    positioning: Positioning,
}

#[derive(Default)]
pub(super) struct PositioningHandler {
    top_sprite_id: WidgetID,
    buttons: HashMap<&'static str, Rc<PosButtonState>>,
    active_button: Option<Rc<PosButtonState>>,
}

impl PositioningHandler {
    pub fn new(panel: &mut EditModeWrapPanel) -> anyhow::Result<Self> {
        let mut buttons = HashMap::new();

        for name in &POS_NAMES {
            let button_id = format!("pos_{name}");
            let component = panel.parser_state.fetch_component_as(&button_id)?;

            let sprite_id = format!("{button_id}_sprite");
            let id = panel.parser_state.get_widget_id(&sprite_id)?;
            let sprite_w = panel
                .layout
                .state
                .widgets
                .get_as::<WidgetSprite>(id)
                .ok_or_else(|| {
                    anyhow::anyhow!("Element with id=\"{sprite_id}\" must be a <sprite>")
                })?;

            let sprite = sprite_w.params.glyph_data.clone().ok_or_else(|| {
                anyhow::anyhow!("Element with id=\"{sprite_id}\" must have a valid src!")
            })?;

            buttons.insert(
                *name,
                Rc::new(PosButtonState {
                    component,
                    name,
                    sprite,
                    positioning: key_to_pos(name),
                }),
            );
        }

        let top_sprite_id = panel.parser_state.get_widget_id("top_pos_sprite")?;
        Ok(Self {
            buttons,
            active_button: None,
            top_sprite_id,
        })
    }

    fn change_highlight(&mut self, common: &mut CallbackDataCommon, key: &str) {
        if let Some(old) = self.active_button.take() {
            old.component.set_sticky_state(common, false);
        }
        let new = self.buttons.get_mut(key).unwrap();
        new.component.set_sticky_state(common, true);
        self.active_button = Some(new.clone());

        // change top sprite
        if let Some(mut sprite) = common
            .state
            .widgets
            .get_as::<WidgetSprite>(self.top_sprite_id)
        {
            sprite.params.glyph_data = Some(new.sprite.clone());
        }
    }

    pub fn pos_button_clicked(
        &mut self,
        common: &mut CallbackDataCommon,
        key: &str,
    ) -> Box<ModifyOverlayTask> {
        self.change_highlight(common, key);

        let pos = key_to_pos(key);
        Box::new(move |app, owc| {
            let state = owc.active_state.as_mut().unwrap(); //want panic
            state.positioning = pos;
            window::save_transform(state, app);
        })
    }

    pub fn reset(&mut self, common: &mut CallbackDataCommon, pos: Positioning) {
        let key = pos_to_key(pos);
        self.change_highlight(common, key);
    }
}

fn key_to_pos(key: &str) -> Positioning {
    match key {
        "static" => Positioning::Static,
        "anchored" => Positioning::Anchored,
        "floating" => Positioning::Floating,
        "hmd" => Positioning::FollowHead { lerp: 1.0 },
        "hand_l" => Positioning::FollowHand {
            hand: LeftRight::Left,
            lerp: 1.0,
        },
        "hand_r" => Positioning::FollowHand {
            hand: LeftRight::Right,
            lerp: 1.0,
        },
        _ => {
            panic!("cannot translate to positioning: {key}")
        }
    }
}

const fn pos_to_key(pos: Positioning) -> &'static str {
    match pos {
        Positioning::Static => "static",
        Positioning::Anchored | Positioning::AnchoredPaused => "anchored",
        Positioning::Floating => "floating",
        Positioning::FollowHead { .. } | Positioning::FollowHeadPaused { .. } => "hmd",
        Positioning::FollowHand {
            hand: LeftRight::Left,
            ..
        }
        | Positioning::FollowHandPaused {
            hand: LeftRight::Left,
            ..
        } => "hand_l",
        Positioning::FollowHand {
            hand: LeftRight::Right,
            ..
        }
        | Positioning::FollowHandPaused {
            hand: LeftRight::Right,
            ..
        } => "hand_r",
    }
}
