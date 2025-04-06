pub mod button;
pub mod label;

use std::{fs::File, sync::Arc};

#[cfg(feature = "wayvr")]
use button::{WayVRAction, WayVRDisplayClickAction};

use glam::Vec4;
use serde::Deserialize;
use vulkano::{command_buffer::CommandBufferUsage, image::view::ImageView};

use crate::{
    backend::common::OverlaySelector, config::AStrMapExt, config_io,
    graphics::dds::WlxCommandBufferDds, state::AppState,
};

use self::{
    button::{modular_button_init, ButtonAction, ButtonData, OverlayAction},
    label::{modular_label_init, LabelContent, LabelData},
};

use super::{
    canvas::{builder::CanvasBuilder, control::Control, Canvas},
    color_parse, GuiColor, FALLBACK_COLOR,
};

type ModularControl = Control<(), ModularData>;
type ExecArgs = Vec<Arc<str>>;

#[derive(Deserialize)]
pub struct ModularUiConfig {
    pub width: f32,
    pub size: [u32; 2],
    pub spawn_pos: Option<[f32; 3]>,
    pub elements: Vec<ModularElement>,
}

#[derive(Deserialize)]
pub struct OverlayListTemplate {
    click_down: Option<OverlayAction>,
    click_up: Option<OverlayAction>,
    long_click_up: Option<OverlayAction>,
    right_down: Option<OverlayAction>,
    right_up: Option<OverlayAction>,
    long_right_up: Option<OverlayAction>,
    middle_down: Option<OverlayAction>,
    middle_up: Option<OverlayAction>,
    long_middle_up: Option<OverlayAction>,
    scroll_down: Option<OverlayAction>,
    scroll_up: Option<OverlayAction>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum ModularElement {
    Panel {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        bg_color: Arc<str>,
    },
    Label {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        #[serde(flatten)]
        data: LabelContent,
    },
    CenteredLabel {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        #[serde(flatten)]
        data: LabelContent,
    },
    Sprite {
        rect: [f32; 4],
        sprite: Arc<str>,
        sprite_st: Option<[f32; 4]>,
    },
    Button {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        text: Arc<str>,
        #[serde(flatten)]
        data: Box<ButtonData>,
    },
    /// Convenience type to save you from having to create a bunch of labels
    BatteryList {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        fg_color_low: Arc<str>,
        fg_color_charging: Arc<str>,
        low_threshold: f32,
        num_devices: usize,
        layout: ListLayout,
    },
    OverlayList {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        layout: ListLayout,
        #[serde(flatten)]
        template: Box<OverlayListTemplate>,
    },
    // Ignored if "wayvr" feature is not enabled
    WayVRLauncher {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        catalog_name: Arc<str>,
    },
    // Ignored if "wayvr" feature is not enabled
    WayVRDisplayList {
        rect: [f32; 4],
        corner_radius: Option<f32>,
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
    },
}

#[derive(Deserialize, Clone)]
pub enum ButtonFunc {
    HideWatch,
    SwitchWatchHand,
}

#[derive(Deserialize)]
pub enum ListLayout {
    Horizontal,
    Vertical,
}

pub enum ModularData {
    Label(Box<LabelData>),
    Button(Box<ButtonData>),
}

#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn modular_canvas(
    size: [u32; 2],
    elements: &[ModularElement],
    state: &mut AppState,
) -> anyhow::Result<Canvas<(), ModularData>> {
    let mut canvas = CanvasBuilder::new(
        size[0] as _,
        size[1] as _,
        state.graphics.clone(),
        state.graphics.native_format,
        (),
    )?;
    let empty_str: Arc<str> = Arc::from("");
    for elem in elements {
        match elem {
            ModularElement::Panel {
                rect: [x, y, w, h],
                corner_radius,
                bg_color,
            } => {
                canvas.bg_color = color_parse(bg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.panel(*x, *y, *w, *h, corner_radius.unwrap_or_default());
            }
            ModularElement::Label {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                data,
            } => {
                canvas.font_size = *font_size;
                canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                let label = canvas.label(
                    *x,
                    *y,
                    *w,
                    *h,
                    corner_radius.unwrap_or_default(),
                    empty_str.clone(),
                );
                modular_label_init(label, data, state);
            }
            ModularElement::CenteredLabel {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                data,
            } => {
                canvas.font_size = *font_size;
                canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                let label = canvas.label_centered(
                    *x,
                    *y,
                    *w,
                    *h,
                    corner_radius.unwrap_or_default(),
                    empty_str.clone(),
                );
                modular_label_init(label, data, state);
            }
            ModularElement::Sprite {
                rect: [x, y, w, h],
                sprite,
                sprite_st,
            } => match sprite_from_path(sprite.clone(), state) {
                Ok(view) => {
                    let sprite = canvas.sprite(*x, *y, *w, *h);
                    sprite.fg_color = Vec4::ONE;
                    sprite.set_sprite(view);

                    let st = sprite_st
                        .map(|st| Vec4::from_slice(&st))
                        .unwrap_or_else(|| Vec4::new(1., 1., 0., 0.));
                    sprite.set_sprite_st(st);
                }
                Err(e) => {
                    log::warn!("Could not load custom UI sprite: {e:?}");
                }
            },
            ModularElement::Button {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                bg_color,
                fg_color,
                text,
                data,
            } => {
                canvas.bg_color = color_parse(bg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.font_size = *font_size;
                let button = canvas.button(
                    *x,
                    *y,
                    *w,
                    *h,
                    corner_radius.unwrap_or_default(),
                    text.clone(),
                );
                modular_button_init(button, data);
            }
            ModularElement::BatteryList {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                fg_color_low,
                fg_color_charging,
                low_threshold,
                num_devices,
                layout,
            } => {
                let num_buttons = *num_devices as f32;
                let mut button_x = *x;
                let mut button_y = *y;
                let low_threshold = low_threshold * 0.01;
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (*w / num_buttons, *h),
                    ListLayout::Vertical => (*w, *h / num_buttons),
                };

                let fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.font_size = *font_size;
                canvas.fg_color = fg_color;

                for i in 0..*num_devices {
                    let label = canvas.label_centered(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        corner_radius.unwrap_or_default(),
                        empty_str.clone(),
                    );
                    modular_label_init(
                        label,
                        &LabelContent::Battery {
                            device: i,
                            low_threshold,
                            low_color: fg_color_low.clone(),
                            charging_color: fg_color_charging.clone(),
                        },
                        state,
                    );

                    button_x += match layout {
                        ListLayout::Horizontal => button_w,
                        ListLayout::Vertical => 0.,
                    };
                    button_y += match layout {
                        ListLayout::Horizontal => 0.,
                        ListLayout::Vertical => button_h,
                    };
                }
            }
            ModularElement::OverlayList {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                bg_color,
                layout,
                template,
            } => {
                let num_buttons = state.screens.len() as f32;
                let mut button_x = *x;
                let mut button_y = *y;
                let (button_w, button_h) = match layout {
                    ListLayout::Horizontal => (*w / num_buttons, *h),
                    ListLayout::Vertical => (*w, *h / num_buttons),
                };

                canvas.bg_color = color_parse(bg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                canvas.font_size = *font_size;

                for screen in &state.screens {
                    let button = canvas.button(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
                        corner_radius.unwrap_or_default(),
                        screen.name.clone(),
                    );

                    // cursed
                    let data = ButtonData {
                        click_down: template.click_down.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        click_up: template.click_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        long_click_up: template.long_click_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        right_down: template.right_down.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        right_up: template.right_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        long_right_up: template.long_right_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        middle_down: template.middle_down.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        middle_up: template.middle_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        long_middle_up: template.long_middle_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        scroll_down: template.scroll_down.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        scroll_up: template.scroll_up.as_ref().map(|f| {
                            vec![ButtonAction::Overlay {
                                target: OverlaySelector::Id(screen.id),
                                action: f.clone(),
                            }]
                        }),
                        ..Default::default()
                    };

                    modular_button_init(button, &data);

                    button_x += match layout {
                        ListLayout::Horizontal => button_w,
                        ListLayout::Vertical => 0.,
                    };
                    button_y += match layout {
                        ListLayout::Horizontal => 0.,
                        ListLayout::Vertical => button_h,
                    };
                }
            }
            #[allow(unused_variables)] // needed in case if wayvr feature is not enabled
            ModularElement::WayVRLauncher {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                bg_color,
                catalog_name,
            } => {
                #[cfg(feature = "wayvr")]
                {
                    if let Some(catalog) = state.session.wayvr_config.get_catalog(catalog_name) {
                        let mut button_x = *x;
                        let button_y = *y;

                        for app in &catalog.apps {
                            let button_w: f32 = *w / catalog.apps.len() as f32;
                            let button_h: f32 = *h;

                            canvas.bg_color = color_parse(bg_color).unwrap_or(*FALLBACK_COLOR);
                            canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                            canvas.font_size = *font_size;

                            let button = canvas.button(
                                button_x + 2.,
                                button_y + 2.,
                                button_w - 4.,
                                button_h - 4.,
                                corner_radius.unwrap_or_default(),
                                Arc::from(app.name.as_str()),
                            );

                            let data = ButtonData {
                                click_up: Some(vec![ButtonAction::WayVR {
                                    action: WayVRAction::AppClick {
                                        catalog_name: catalog_name.clone(),
                                        app_name: Arc::from(app.name.as_str()),
                                    },
                                }]),
                                ..Default::default()
                            };

                            modular_button_init(button, &data);
                            button_x += button_w;
                        }
                    } else {
                        log::error!("WayVR catalog \"{catalog_name}\" not found");
                    }
                }
                #[cfg(not(feature = "wayvr"))]
                {
                    log::error!("WayVR feature is not enabled, ignoring");
                }
            }
            #[allow(unused_variables)]
            ModularElement::WayVRDisplayList {
                rect: [x, y, w, h],
                corner_radius,
                font_size,
                fg_color,
                bg_color,
            } => {
                #[cfg(feature = "wayvr")]
                {
                    let mut button_x = *x;
                    let button_y = *y;
                    let displays = &state.session.wayvr_config.displays;
                    for (display_name, display) in displays {
                        let button_w: f32 = (*w / displays.len() as f32).min(80.0);
                        let button_h: f32 = *h;

                        canvas.bg_color = color_parse(bg_color).unwrap_or(*FALLBACK_COLOR);
                        canvas.fg_color = color_parse(fg_color).unwrap_or(*FALLBACK_COLOR);
                        canvas.font_size = *font_size;

                        let button = canvas.button(
                            button_x + 2.,
                            button_y + 2.,
                            button_w - 4.,
                            button_h - 4.,
                            corner_radius.unwrap_or_default(),
                            Arc::from(display_name.as_str()),
                        );

                        let data = ButtonData {
                            click_up: Some(vec![ButtonAction::WayVR {
                                action: WayVRAction::DisplayClick {
                                    display_name: Arc::from(display_name.as_str()),
                                    action: WayVRDisplayClickAction::ToggleVisibility,
                                },
                            }]),
                            long_click_up: Some(vec![ButtonAction::WayVR {
                                action: WayVRAction::DisplayClick {
                                    display_name: Arc::from(display_name.as_str()),
                                    action: WayVRDisplayClickAction::Reset,
                                },
                            }]),
                            ..Default::default()
                        };

                        modular_button_init(button, &data);
                        button_x += button_w;
                    }
                }
                #[cfg(not(feature = "wayvr"))]
                {
                    log::error!("WayVR feature is not enabled, ignoring")
                }
            }
        }
    }
    Ok(canvas.build())
}

pub fn color_parse_or_default(color: &str) -> GuiColor {
    color_parse(color).unwrap_or_else(|e| {
        log::error!("Failed to parse color '{color}': {e}");
        *FALLBACK_COLOR
    })
}

fn sprite_from_path(path: Arc<str>, app: &mut AppState) -> anyhow::Result<Arc<ImageView>> {
    if let Some(view) = app.sprites.arc_get(&path) {
        return Ok(view.clone());
    }

    let real_path = config_io::get_config_root().join(&*path);

    let Ok(f) = File::open(real_path) else {
        anyhow::bail!("Could not open custom sprite at: {}", path);
    };

    let mut command_buffer = app
        .graphics
        .create_uploads_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

    match command_buffer.texture2d_dds(f) {
        Ok(image) => {
            command_buffer.build_and_execute_now()?;
            Ok(ImageView::new_default(image)?)
        }
        Err(e) => {
            anyhow::bail!("Could not use custom sprite at: {}\n{:?}", path, e);
        }
    }
}
