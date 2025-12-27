use crate::overlays::edit::{
    EditModeWrapPanel,
    sprite_tab::{SpriteTabHandler, SpriteTabKey},
};
use wlx_common::overlays::{BackendAttribValue, MouseTransform};

static MOUSE_NAMES: [&str; 9] = [
    "default",
    "normal",
    "rotate90",
    "rotate180",
    "rotate270",
    "flipped",
    "flip90",
    "flip180",
    "flip270",
];

pub fn new_mouse_tab_handler(
    panel: &mut EditModeWrapPanel,
) -> anyhow::Result<SpriteTabHandler<MouseTransform>> {
    SpriteTabHandler::new(
        panel,
        "mouse",
        &MOUSE_NAMES,
        Box::new(|_common, state| {
            let mouse_transform = *state;
            Box::new(move |app, owc| {
                owc.backend
                    .set_attrib(app, BackendAttribValue::MouseTransform(mouse_transform));
            })
        }),
        None,
    )
}

impl SpriteTabKey for MouseTransform {
    fn to_tab_key(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Normal => "normal",
            Self::Rotated90 => "rotate90",
            Self::Rotated180 => "rotate180",
            Self::Rotated270 => "rotate270",
            Self::Flipped => "flipped",
            Self::Flipped90 => "flip90",
            Self::Flipped180 => "flip180",
            Self::Flipped270 => "flip270",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "default" => Self::Default,
            "normal" => Self::Normal,
            "rotate90" => Self::Rotated90,
            "rotate180" => Self::Rotated180,
            "rotate270" => Self::Rotated270,
            "flipped" => Self::Flipped,
            "flip90" => Self::Flipped90,
            "flip180" => Self::Flipped180,
            "flip270" => Self::Flipped270,
            _ => {
                panic!("cannot translate to mouse transform: {key}")
            }
        }
    }
}
