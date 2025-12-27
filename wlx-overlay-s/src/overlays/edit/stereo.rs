use crate::overlays::edit::{
    EditModeWrapPanel,
    sprite_tab::{SpriteTabHandler, SpriteTabKey},
};
use wlx_common::overlays::{BackendAttribValue, StereoMode};

static STEREO_NAMES: [&str; 5] = ["none", "leftright", "rightleft", "topbottom", "bottomtop"];

pub fn new_stereo_tab_handler(
    panel: &mut EditModeWrapPanel,
) -> anyhow::Result<SpriteTabHandler<StereoMode>> {
    SpriteTabHandler::new(
        panel,
        "stereo",
        &STEREO_NAMES,
        Box::new(|_common, state| {
            let stereo = *state;
            Box::new(move |app, owc| {
                owc.backend
                    .set_attrib(app, BackendAttribValue::Stereo(stereo));
            })
        }),
        None,
    )
}

impl SpriteTabKey for StereoMode {
    fn to_tab_key(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LeftRight => "leftright",
            Self::RightLeft => "rightleft",
            Self::TopBottom => "topbottom",
            Self::BottomTop => "bottomtop",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "none" => Self::None,
            "leftright" => Self::LeftRight,
            "rightleft" => Self::RightLeft,
            "topbottom" => Self::TopBottom,
            "bottomtop" => Self::BottomTop,
            _ => {
                panic!("cannot translate to stereo mode: {key}")
            }
        }
    }
}
