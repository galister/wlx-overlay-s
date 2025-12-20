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
            let mouse_transform = state.clone();
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
            MouseTransform::Default => "default",
            MouseTransform::Normal => "normal",
            MouseTransform::Rotated90 => "rotate90",
            MouseTransform::Rotated180 => "rotate180",
            MouseTransform::Rotated270 => "rotate270",
            MouseTransform::Flipped => "flipped",
            MouseTransform::Flipped90 => "flip90",
            MouseTransform::Flipped180 => "flip180",
            MouseTransform::Flipped270 => "flip270",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "default" => MouseTransform::Default,
            "normal" => MouseTransform::Normal,
            "rotate90" => MouseTransform::Rotated90,
            "rotate180" => MouseTransform::Rotated180,
            "rotate270" => MouseTransform::Rotated270,
            "flipped" => MouseTransform::Flipped,
            "flip90" => MouseTransform::Flipped90,
            "flip180" => MouseTransform::Flipped180,
            "flip270" => MouseTransform::Flipped270,
            _ => {
                panic!("cannot translate to mouse transform: {key}")
            }
        }
    }
}
