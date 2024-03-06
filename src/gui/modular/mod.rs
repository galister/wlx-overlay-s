pub mod button;
pub mod label;
//pub mod slider;

use std::sync::Arc;

use glam::Vec3;
use serde::Deserialize;

use crate::{backend::common::OverlaySelector, state::AppState};

use self::{
    button::{modular_button_init, ButtonAction, ButtonData, OverlayAction},
    label::{modular_label_init, LabelContent, LabelData},
};

use super::{color_parse, Canvas, CanvasBuilder, Control};

type ModularControl = Control<(), ModularData>;
type ExecArgs = Vec<Arc<str>>;

const FALLBACK_COLOR: Vec3 = Vec3 {
    x: 1.,
    y: 0.,
    z: 1.,
};

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
        bg_color: Arc<str>,
    },
    Label {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        #[serde(flatten)]
        data: LabelContent,
    },
    Button {
        rect: [f32; 4],
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        text: Arc<str>,
        #[serde(flatten)]
        data: ButtonData,
    },
    /// Convenience type to save you from having to create a bunch of labels
    BatteryList {
        rect: [f32; 4],
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
        font_size: isize,
        fg_color: Arc<str>,
        bg_color: Arc<str>,
        layout: ListLayout,
        #[serde(flatten)]
        template: OverlayListTemplate,
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
    Label(LabelData),
    Button(ButtonData),
}

pub fn modular_canvas(
    size: &[u32; 2],
    elements: &[ModularElement],
    state: &AppState,
) -> anyhow::Result<Canvas<(), ModularData>> {
    let mut canvas = CanvasBuilder::new(
        size[0] as _,
        size[1] as _,
        state.graphics.clone(),
        state.format,
        (),
    )?;
    let empty_str: Arc<str> = Arc::from("");
    for elem in elements.iter() {
        match elem {
            ModularElement::Panel {
                rect: [x, y, w, h],
                bg_color,
            } => {
                canvas.bg_color = color_parse(bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.panel(*x, *y, *w, *h);
            }
            ModularElement::Label {
                rect: [x, y, w, h],
                font_size,
                fg_color,
                data,
            } => {
                canvas.font_size = *font_size;
                canvas.fg_color = color_parse(fg_color).unwrap_or(FALLBACK_COLOR);
                let label = canvas.label(*x, *y, *w, *h, empty_str.clone());
                modular_label_init(label, data);
            }
            ModularElement::Button {
                rect: [x, y, w, h],
                font_size,
                bg_color,
                fg_color,
                text,
                data,
            } => {
                canvas.bg_color = color_parse(bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = *font_size;
                let button = canvas.button(*x, *y, *w, *h, text.clone());
                modular_button_init(button, data);
            }
            ModularElement::BatteryList {
                rect: [x, y, w, h],
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

                let fg_color = color_parse(fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = *font_size;
                canvas.fg_color = fg_color;

                for i in 0..*num_devices {
                    let label = canvas.label_centered(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
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

                canvas.bg_color = color_parse(bg_color).unwrap_or(FALLBACK_COLOR);
                canvas.fg_color = color_parse(fg_color).unwrap_or(FALLBACK_COLOR);
                canvas.font_size = *font_size;

                for screen in state.screens.iter() {
                    let button = canvas.button(
                        button_x + 2.,
                        button_y + 2.,
                        button_w - 4.,
                        button_h - 4.,
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
        }
    }
    Ok(canvas.build())
}

pub fn color_parse_or_default(color: &str) -> Vec3 {
    color_parse(color).unwrap_or_else(|e| {
        log::error!("Failed to parse color '{}': {}", color, e);
        FALLBACK_COLOR
    })
}
