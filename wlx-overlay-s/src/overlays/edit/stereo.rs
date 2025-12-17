use crate::{
    overlays::edit::{
        EditModeWrapPanel,
        sprite_tab::{SpriteTabHandler, SpriteTabKey},
    },
    windowing::backend::{BackendAttribValue, StereoMode},
};

static STEREO_NAMES: [&str; 5] = ["none", "leftright", "rightleft", "topbottom", "bottomtop"];

pub fn new_stereo_tab_handler(
    panel: &mut EditModeWrapPanel,
) -> anyhow::Result<SpriteTabHandler<StereoMode>> {
    SpriteTabHandler::new(
        panel,
        "stereo",
        &STEREO_NAMES,
        Box::new(|_common, state| {
            let stereo = state.clone();
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
            StereoMode::None => "none",
            StereoMode::LeftRight => "leftright",
            StereoMode::RightLeft => "rightleft",
            StereoMode::TopBottom => "topbottom",
            StereoMode::BottomTop => "bottomtop",
        }
    }

    fn from_tab_key(key: &str) -> Self {
        match key {
            "none" => StereoMode::None,
            "leftright" => StereoMode::LeftRight,
            "rightleft" => StereoMode::RightLeft,
            "topbottom" => StereoMode::TopBottom,
            "bottomtop" => StereoMode::BottomTop,
            _ => {
                panic!("cannot translate to positioning: {key}")
            }
        }
    }
}
