use glam::{Affine3A, Quat, Vec3A};
use wgui::{
    i18n::Translation,
    parser::parse_color_hex,
    renderer_vk::text::TextStyle,
    taffy::{
        self,
        prelude::{auto, length, percent},
    },
    widget::{
        rectangle::{Rectangle, RectangleParams},
        text::{TextLabel, TextParams},
        util::WLength,
    },
};

use crate::{
    backend::overlay::{OverlayBackend, OverlayState, Z_ORDER_TOAST},
    gui::panel::GuiPanel,
    state::AppState,
};

const FONT_SIZE: isize = 16;
const PADDING: (f32, f32) = (25., 7.);
const PIXELS_TO_METERS: f32 = 1. / 2000.;

#[allow(clippy::too_many_lines)]
fn new_tooltip(
    text: &str,
    transform: Affine3A,
    app: &mut AppState,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    let mut panel = GuiPanel::new_blank(app, ()).ok()?;

    let globals = panel.layout.state.globals.clone();
    let mut i18n = globals.i18n();

    let (rect, _) = panel
        .layout
        .add_child(
            panel.layout.root_widget,
            Rectangle::create(RectangleParams {
                color: parse_color_hex("#1e2030").unwrap(),
                border_color: parse_color_hex("#5e7090").unwrap(),
                border: 1.0,
                round: WLength::Units(4.0),
                ..Default::default()
            })
            .unwrap(),
            taffy::Style {
                align_items: Some(taffy::AlignItems::Center),
                justify_content: Some(taffy::JustifyContent::Center),
                flex_direction: taffy::FlexDirection::Column,
                padding: length(4.0),
                ..Default::default()
            },
        )
        .ok()?;

    let _ = panel.layout.add_child(
        rect.id,
        TextLabel::create(
            &mut i18n,
            TextParams {
                content: Translation::from_raw_text(text),
                style: TextStyle {
                    color: parse_color_hex("#ffffff"),
                    ..Default::default()
                },
            },
        )
        .unwrap(),
        taffy::Style {
            size: taffy::Size {
                width: percent(1.0),
                height: auto(),
            },
            padding: length(8.0),
            ..Default::default()
        },
    );

    panel.update_layout().ok()?;

    let state = OverlayState {
        name: "tooltip".into(),
        want_visible: true,
        spawn_scale: panel.layout.content_size.x * PIXELS_TO_METERS,
        spawn_rotation: Quat::IDENTITY,
        spawn_point: Vec3A::ZERO,
        z_order: Z_ORDER_TOAST,
        positioning: crate::backend::overlay::Positioning::Static,
        ..Default::default()
    };
    let backend = Box::new(panel);

    Some((state, backend))
}
