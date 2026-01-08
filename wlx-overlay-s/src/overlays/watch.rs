use std::{rc::Rc, time::Duration};

use anyhow::Context;
use glam::{Affine3A, Quat, Vec3, Vec3A, vec3};
use wgui::{
    assets::AssetPath,
    components::button::ComponentButton,
    event::{CallbackDataCommon, EventAlterables, StyleSetRequest},
    layout::WidgetID,
    parser::{Fetchable, ParseDocumentParams,parse_color_hex},
    renderer_vk::text::custom_glyph::CustomGlyphData,
    taffy,
    widget::{
        image::WidgetImage, label::WidgetLabel, rectangle::WidgetRectangle, sprite::WidgetSprite,
    },    
    i18n::Translation,

};

use wlx_common::{
    common::LeftRight,
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
        backend::task::ModifyPanelCommand,
    gui::{
        panel::{
            GuiPanel, NewGuiPanelParams, device_list::DeviceList, overlay_list::OverlayList,
            set_list::SetList,
        },
        timer::GuiTimer,
    },
    state::AppState,
    windowing::{
        Z_ORDER_WATCH,
        backend::OverlayEventData,
        window::{OverlayWindowConfig, OverlayWindowData},
    },
};

pub const WATCH_NAME: &str = "watch";
const MAX_TOOLBOX_BUTTONS: usize = 16;
const MAX_DEVICES: usize = 12;

pub const WATCH_POS: Vec3 = vec3(-0.03, -0.01, 0.125);
pub const WATCH_ROT: Quat = Quat::from_xyzw(-0.707_106_6, 0.000_796_361_8, 0.707_106_6, 0.0);

struct OverlayButton {
    button: Rc<ComponentButton>,
    label: WidgetID,
    sprite: WidgetID,
    condensed: bool,
}

#[derive(Default)]
struct WatchState {
    edit_mode_widgets: Vec<(WidgetID, bool)>,
    edit_add_widget: WidgetID,
    device_list: DeviceList,
    overlay_list: OverlayList,
    set_list: SetList,
    clock_12h: bool,
}

pub fn create_watch(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let state = WatchState {
        clock_12h: app.session.config.clock_12h,
        ..Default::default()
    };
    let watch_xml = "gui/watch.xml";

    let mut panel =
        GuiPanel::new_from_template(app, watch_xml, state, NewGuiPanelParams::default())?;

    let mut alterables = EventAlterables::default();
    sets_or_overlays(&panel, app, &mut alterables);
    panel.layout.process_alterables(alterables)?;

    let doc_params = ParseDocumentParams {
        globals: panel.layout.state.globals.clone(),
        path: AssetPath::FileOrBuiltIn(watch_xml),
        extra: panel.doc_extra.take().unwrap_or_default(),
    };

    panel.on_notify = Some(Box::new({
        let name=WATCH_NAME;
        move |panel, app, event_data| {
        let mut alterables = EventAlterables::default();

        let mut elems_changed = panel.state.overlay_list.on_notify(
            &mut panel.layout,
            &mut panel.parser_state,
            &event_data,
            &mut alterables,
            &doc_params,
        )?;

        elems_changed |= panel.state.set_list.on_notify(
            &mut panel.layout,
            &mut panel.parser_state,
            &event_data,
            &mut alterables,
            &doc_params,
        )?;

        elems_changed |= panel.state.device_list.on_notify(
            app,
            &mut panel.layout,
            &mut panel.parser_state,
            &event_data,
            &doc_params,
        )?;



        match event_data {
            OverlayEventData::EditModeChanged(edit_mode) => {
                if let Ok(btn_edit_mode) = panel
                    .parser_state
                    .fetch_component_as::<ComponentButton>("btn_edit_mode")
                {
                    let mut com = CallbackDataCommon {
                        alterables: &mut alterables,
                        state: &panel.layout.state,
                    };
                    btn_edit_mode.set_sticky_state(&mut com, edit_mode);
                }
            }
            OverlayEventData::SettingsChanged => {
                panel.layout.mark_redraw();
                sets_or_overlays(panel, app, &mut alterables);

                if app.session.config.clock_12h != panel.state.clock_12h {
                    panel.state.clock_12h = app.session.config.clock_12h;

                    let clock_root = panel.parser_state.get_widget_id("clock_root")?;
                    panel.layout.remove_children(clock_root);

                    panel.parser_state.instantiate_template(
                        &doc_params,
                        "Clock",
                        &mut panel.layout,
                        clock_root,
                        Default::default(),
                    )?;

                    elems_changed = true;
                }
            }
            OverlayEventData::CustomCommand {element, command} =>{
                if let Err(e) = apply_custom_command(panel, app, &element, &command) {
                    log::warn!("Could not apply {command:?} on {name}/{element}: {e:?}");
                }
            }
            _ => {}
        }


        if elems_changed {
            panel.process_custom_elems(app);
        }

        panel.layout.process_alterables(alterables)?;
        Ok(())
    }}));

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: LeftRight::Left,
        lerp: 1.0,
        align_to_hmd: false,
    };

    panel.update_layout(app)?;

    Ok(OverlayWindowConfig {
        name: WATCH_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                WATCH_ROT,
                WATCH_POS,
            ),
            ..OverlayWindowState::default()
        },
        show_on_spawn: true,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

fn sets_or_overlays(
    panel: &GuiPanel<WatchState>,
    app: &mut AppState,
    alterables: &mut EventAlterables,
) {
    let display = if app.session.config.sets_on_watch {
        [taffy::Display::None, taffy::Display::Flex]
    } else {
        [taffy::Display::Flex, taffy::Display::None]
    };

    let widget = [
        panel
            .parser_state
            .get_widget_id("panels_root")
            .unwrap_or_default(),
        panel
            .parser_state
            .get_widget_id("sets_root")
            .unwrap_or_default(),
    ];

    for i in 0..2 {
        alterables.set_style(widget[i], StyleSetRequest::Display(display[i]));
    }
}

pub fn watch_fade<D>(app: &mut AppState, watch: &mut OverlayWindowData<D>) {
    let Some(state) = watch.config.active_state.as_mut() else {
        return;
    };

    let to_hmd = (state.transform.translation - app.input_state.hmd.translation).normalize();
    let watch_normal = state.transform.transform_vector3a(Vec3A::NEG_Z).normalize();
    let dot = to_hmd.dot(watch_normal);

    state.alpha = (dot - app.session.config.watch_view_angle_min)
        / (app.session.config.watch_view_angle_max - app.session.config.watch_view_angle_min);
    state.alpha += 0.1;
    state.alpha = state.alpha.clamp(0., 1.);
}
fn apply_custom_command(
    panel: &mut GuiPanel<WatchState>,
    app: &mut AppState,
    element: &str,
    command: &ModifyPanelCommand,
) -> anyhow::Result<()> {
    let mut alterables = EventAlterables::default();
    let mut com = CallbackDataCommon {
        alterables: &mut alterables,
        state: &panel.layout.state,
    };

    match command {
        ModifyPanelCommand::SetText(text) => {
            if let Ok(mut label) = panel
                .parser_state
                .fetch_widget_as::<WidgetLabel>(&panel.layout.state, element)
            {
                label.set_text(&mut com, Translation::from_raw_text(text));
            } else if let Ok(button) = panel
                .parser_state
                .fetch_component_as::<ComponentButton>(element)
            {
                button.set_text(&mut com, Translation::from_raw_text(text));
            } else {
                anyhow::bail!("No <label> or <Button> with such id.");
            }
        }
        ModifyPanelCommand::SetImage(path) => {
            if let Ok(pair) = panel
                .parser_state
                .fetch_widget(&panel.layout.state, element)
            {
                let data = CustomGlyphData::from_assets(
                    &app.wgui_globals,
                    wgui::assets::AssetPath::File(path),
                )
                .context("Could not load content from supplied path.")?;

                if let Some(mut sprite) = pair.widget.get_as::<WidgetSprite>() {
                    sprite.set_content(&mut com, Some(data));
                } else if let Some(mut image) = pair.widget.get_as::<WidgetImage>() {
                    image.set_content(&mut com, Some(data));
                } else {
                    anyhow::bail!("No <sprite> or <image> with such id.");
                }
            } else {
                anyhow::bail!("No <sprite> or <image> with such id.");
            }
        }
        ModifyPanelCommand::SetColor(color) => {
            let color = parse_color_hex(color)
                .context("Invalid color format, must be a html hex color!")?;

            if let Ok(pair) = panel
                .parser_state
                .fetch_widget(&panel.layout.state, element)
            {
                if let Some(mut rect) = pair.widget.get_as::<WidgetRectangle>() {
                    rect.set_color(&mut com, color);
                } else if let Some(mut label) = pair.widget.get_as::<WidgetLabel>() {
                    label.set_color(&mut com, color, true);
                } else if let Some(mut sprite) = pair.widget.get_as::<WidgetSprite>() {
                    sprite.set_color(&mut com, color);
                } else {
                    anyhow::bail!("No <rectangle> or <label> or <sprite> with such id.");
                }
            } else {
                anyhow::bail!("No <rectangle> or <label> or <sprite> with such id.");
            }
        }
        ModifyPanelCommand::SetVisible(visible) => {
            let wid = panel
                .parser_state
                .get_widget_id(element)
                .context("No widget with such id.")?;

            let display = if *visible {
                taffy::Display::Flex
            } else {
                taffy::Display::None
            };

            com.alterables
                .set_style(wid, wgui::event::StyleSetRequest::Display(display));
        }
        ModifyPanelCommand::SetStickyState(sticky_down) => {
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

