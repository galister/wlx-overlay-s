use std::{sync::Arc, time::Duration};

use anyhow::Context;
use glam::{Affine3A, Quat, Vec3, vec3};
use wgui::{
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables},
    i18n::Translation,
    parser::{Fetchable, parse_color_hex},
    renderer_vk::text::custom_glyph::{CustomGlyphContent, CustomGlyphData},
    taffy,
    widget::{label::WidgetLabel, rectangle::WidgetRectangle, sprite::WidgetSprite},
};
use wlx_common::windowing::OverlayWindowState;

use crate::{
    backend::task::OverlayCustomCommand,
    gui::{
        panel::{GuiPanel, NewGuiPanelParams},
        timer::GuiTimer,
    },
    state::AppState,
    windowing::{
        backend::OverlayEventData,
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

struct CustomPanelState {}

pub fn create_custom(app: &mut AppState, name: Arc<str>) -> Option<OverlayWindowConfig> {
    let params = NewGuiPanelParams {
        external_xml: true,
        ..NewGuiPanelParams::default()
    };

    let mut panel =
        GuiPanel::new_from_template(app, &format!("gui/{name}.xml"), CustomPanelState {}, params)
            .inspect_err(|e| log::warn!("Error creating '{name}': {e:?}"))
            .ok()?;

    panel
        .update_layout()
        .inspect_err(|e| log::warn!("Error layouting '{name}': {e:?}"))
        .ok()?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let scale = panel.layout.content_size.x / 40.0 * 0.05;

    panel.on_notify = Some(Box::new({
        let name = name.clone();
        move |panel, app, event_data| {
            let OverlayEventData::CustomCommand { element, command } = event_data else {
                return Ok(());
            };

            if let Err(e) = apply_custom_command(panel, app, &element, &command) {
                log::warn!("Could not apply {command:?} on {name}/{element}: {e:?}");
            };

            Ok(())
        }
    }));

    Some(OverlayWindowConfig {
        name,
        category: OverlayCategory::Panel,
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * scale,
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.5),
            ),
            ..OverlayWindowState::default()
        },
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

fn apply_custom_command(
    panel: &mut GuiPanel<CustomPanelState>,
    app: &mut AppState,
    element: &str,
    command: &OverlayCustomCommand,
) -> anyhow::Result<()> {
    let mut alterables = EventAlterables::default();
    let mut com = CallbackDataCommon {
        alterables: &mut alterables,
        state: &panel.layout.state,
    };

    match command {
        OverlayCustomCommand::SetText(text) => {
            if let Ok(mut label) = panel
                .parser_state
                .fetch_widget_as::<WidgetLabel>(&panel.layout.state, element)
            {
                label.set_text(&mut com, Translation::from_raw_text(text));
            } else if let Ok(button) = panel
                .parser_state
                .fetch_component_as::<ComponentButton>(&element)
            {
                button.set_text(&mut com, Translation::from_raw_text(text));
            } else {
                anyhow::bail!("No <label> or <Button> with such id.");
            }
        }
        OverlayCustomCommand::SetSprite(path) => {
            let mut widget = panel
                .parser_state
                .fetch_widget_as::<WidgetSprite>(&panel.layout.state, element)
                .context("No <sprite> with such id.")?;

            if path == "none" {
                widget.set_content(&mut com, None);
            } else {
                let content = CustomGlyphContent::from_assets(
                    &mut app.wgui_globals,
                    wgui::assets::AssetPath::File(&path),
                )
                .context("Could not load content from supplied path.")?;

                let data = CustomGlyphData::new(content);

                widget.set_content(&mut com, Some(data));
            }
        }
        OverlayCustomCommand::SetColor(color) => {
            let color = parse_color_hex(&color)
                .context("Invalid color format, must be a html hex color!")?;

            if let Ok(pair) = panel
                .parser_state
                .fetch_widget(&panel.layout.state, element)
            {
                if let Some(mut rect) = pair.widget.get_as_mut::<WidgetRectangle>() {
                    rect.set_color(&mut com, color);
                } else if let Some(mut label) = pair.widget.get_as_mut::<WidgetLabel>() {
                    label.set_color(&mut com, color, true);
                } else if let Some(mut sprite) = pair.widget.get_as_mut::<WidgetSprite>() {
                    sprite.set_color(&mut com, color);
                } else {
                    anyhow::bail!("No <rectangle> or <label> or <sprite> with such id.");
                }
            } else {
                anyhow::bail!("No <rectangle> or <label> or <sprite> with such id.");
            }
        }
        OverlayCustomCommand::SetVisible(visible) => {
            let wid = panel
                .parser_state
                .get_widget_id(&element)
                .context("No widget with such id.")?;

            let display = if *visible {
                taffy::Display::Flex
            } else {
                taffy::Display::None
            };

            com.alterables
                .set_style(wid, wgui::event::StyleSetRequest::Display(display));
        }
        OverlayCustomCommand::SetStickyState(sticky_down) => {
            let button = panel
                .parser_state
                .fetch_component_as::<ComponentButton>(element)
                .context("No <Button> with such id.")?;
            button.set_sticky_state(&mut com, *sticky_down);
        }
    }

    panel.layout.process_alterables(alterables)?;
    Ok(())
}
