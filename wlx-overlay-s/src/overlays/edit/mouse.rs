use wlx_capture::frame::Transform;

use crate::{
    overlays::edit::{
        EditModeWrapPanel,
        sprite_tab::{SpriteTabHandler, SpriteTabKey},
    },
    windowing::backend::BackendAttribValue,
};

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
) -> anyhow::Result<SpriteTabHandler<Transform>> {
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

impl SpriteTabKey for Transform {
    fn to_tab_key(&self) -> &'static str {
        match self {
            Transform::Undefined => "default",
            Transform::Normal => "normal",
            Transform::Rotated90 => "rotate90",
            Transform::Rotated180 => "rotate180",
            Transform::Rotated270 => "rotate270",
            Transform::Flipped => "flipped",
            Transform::Flipped90 => "flip90",
            Transform::Flipped180 => "flip180",
            Transform::Flipped270 => "flip270",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "default" => Transform::Undefined,
            "normal" => Transform::Normal,
            "rotate90" => Transform::Rotated90,
            "rotate180" => Transform::Rotated180,
            "rotate270" => Transform::Rotated270,
            "flipped" => Transform::Flipped,
            "flip90" => Transform::Flipped90,
            "flip180" => Transform::Flipped180,
            "flip270" => Transform::Flipped270,
            _ => {
                panic!("cannot translate to mouse transform: {key}")
            }
        }
    }
}
