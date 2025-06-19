use glam::{Vec3A, vec2};
use std::sync::{Arc, LazyLock};
use wgui::parser::parse_color_hex;
use wgui::renderer_vk::text::{FontWeight, TextStyle};
use wgui::taffy;
use wgui::taffy::prelude::{length, percent};
use wgui::widget::rectangle::{Rectangle, RectangleParams};
use wgui::widget::text::{TextLabel, TextParams};
use wgui::widget::util::WLength;

use crate::backend::overlay::{OverlayData, OverlayState, Positioning, Z_ORDER_ANCHOR};
use crate::gui::panel::GuiPanel;
use crate::state::AppState;

pub static ANCHOR_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("anchor"));

pub fn create_anchor<O>(app: &mut AppState) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let mut panel = GuiPanel::new_blank(app, 200)?;

    let (rect, _) = panel.layout.add_child(
        panel.layout.root_widget,
        Rectangle::create(RectangleParams {
            color: wgui::drawing::Color::new(0., 0., 0., 0.),
            border_color: parse_color_hex("#ffff00").unwrap(),
            border: 2.0,
            round: WLength::Percent(1.0),
            ..Default::default()
        })
        .unwrap(),
        taffy::Style {
            size: taffy::Size {
                width: percent(1.0),
                height: percent(1.0),
            },
            align_items: Some(taffy::AlignItems::Center),
            justify_content: Some(taffy::JustifyContent::Center),
            padding: length(4.0),
            ..Default::default()
        },
    )?;

    let _ = panel.layout.add_child(
        rect,
        TextLabel::create(TextParams {
            content: "Center".into(),
            style: TextStyle {
                weight: Some(FontWeight::Bold),
                size: Some(36.0),
                color: parse_color_hex("#ffff00"),
                ..Default::default()
            },
        })
        .unwrap(),
        taffy::style::Style::DEFAULT,
    );

    panel.layout.update(vec2(2048., 2048.), 0.0)?;

    Ok(OverlayData {
        state: OverlayState {
            name: ANCHOR_NAME.clone(),
            want_visible: false,
            interactable: false,
            grabbable: false,
            z_order: Z_ORDER_ANCHOR,
            spawn_scale: 0.1,
            spawn_point: Vec3A::NEG_Z * 0.5,
            positioning: Positioning::Static,
            ..Default::default()
        },
        backend: Box::new(panel),
        ..Default::default()
    })
}
