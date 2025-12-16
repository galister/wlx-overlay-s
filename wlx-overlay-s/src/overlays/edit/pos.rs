use wgui::{event::StyleSetRequest, parser::Fetchable, taffy};
use wlx_common::{common::LeftRight, windowing::Positioning};

use crate::{
    overlays::edit::{
        sprite_tab::{SpriteTabHandler, SpriteTabKey},
        EditModeWrapPanel,
    },
    windowing::window,
};

static POS_NAMES: [&str; 6] = ["static", "anchored", "floating", "hmd", "hand_l", "hand_r"];

#[derive(Default)]
pub struct PosTabState {
    pos: Positioning,
    has_lerp: bool,
}

impl From<Positioning> for PosTabState {
    fn from(value: Positioning) -> Self {
        Self {
            pos: value,
            has_lerp: false,
        }
    }
}

pub fn new_pos_tab_handler(
    panel: &mut EditModeWrapPanel,
) -> anyhow::Result<SpriteTabHandler<PosTabState>> {
    let interpolation_id = panel.parser_state.get_widget_id("pos_interpolation")?;

    SpriteTabHandler::new(
        panel,
        "pos",
        &POS_NAMES,
        Box::new(|_common, state| {
            let positioning = state.pos;
            Box::new(move |app, owc| {
                let state = owc.active_state.as_mut().unwrap(); //want panic
                state.positioning = positioning;
                window::save_transform(state, app);
            })
        }),
        Some(Box::new(move |common, state| {
            let interpolation_disp = if state.has_lerp {
                taffy::Display::Flex
            } else {
                taffy::Display::None
            };

            common.alterables.set_style(
                interpolation_id,
                StyleSetRequest::Display(interpolation_disp),
            );
        })),
    )
}

impl SpriteTabKey for PosTabState {
    fn to_tab_key(&self) -> &'static str {
        match self.pos {
            Positioning::Static => "static",
            Positioning::Anchored => "anchored",
            Positioning::Floating => "floating",
            Positioning::FollowHead { .. } => "hmd",
            Positioning::FollowHand {
                hand: LeftRight::Left,
                ..
            } => "hand_l",
            Positioning::FollowHand {
                hand: LeftRight::Right,
                ..
            } => "hand_r",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "static" => PosTabState {
                pos: Positioning::Static,
                has_lerp: false,
            },
            "anchored" => PosTabState {
                pos: Positioning::Anchored,
                has_lerp: false,
            },
            "floating" => PosTabState {
                pos: Positioning::Floating,
                has_lerp: false,
            },
            "hmd" => PosTabState {
                pos: Positioning::FollowHead { lerp: 1.0 },
                has_lerp: true,
            },
            "hand_l" => PosTabState {
                pos: Positioning::FollowHand {
                    hand: LeftRight::Left,
                    lerp: 1.0,
                },
                has_lerp: true,
            },
            "hand_r" => PosTabState {
                pos: Positioning::FollowHand {
                    hand: LeftRight::Right,
                    lerp: 1.0,
                },
                has_lerp: true,
            },
            _ => {
                panic!("cannot translate to positioning: {key}")
            }
        }
    }
}
