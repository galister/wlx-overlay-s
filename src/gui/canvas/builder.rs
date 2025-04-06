use glam::Vec4;
use std::sync::Arc;

use vulkano::format::Format;

use crate::{
    graphics::WlxGraphics,
    gui::{canvas::control::ControlRenderer, GuiColor, KeyCapType},
};

use super::{control::Control, Canvas, Rect};

pub struct CanvasBuilder<D, S> {
    canvas: Canvas<D, S>,

    pub fg_color: GuiColor,
    pub bg_color: GuiColor,
    pub font_size: isize,
}

impl<D, S> CanvasBuilder<D, S> {
    pub fn new(
        width: usize,
        height: usize,
        graphics: Arc<WlxGraphics>,
        format: Format,
        data: D,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            canvas: Canvas::new(width, height, graphics, format, data)?,
            bg_color: Vec4::ZERO,
            fg_color: Vec4::ONE,
            font_size: 16,
        })
    }

    pub fn build(self) -> Canvas<D, S> {
        self.canvas
    }

    // Creates a panel with bg_color inherited from the canvas
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: radius,
            bg_color: self.bg_color,
            on_render_bg: Some(Control::render_rounded_rect),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a label with fg_color, font_size inherited from the canvas
    pub fn label(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        text: Arc<str>,
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: radius,
            text,
            fg_color: self.fg_color,
            size: self.font_size,
            on_render_fg: Some(Control::render_text),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a label with fg_color, font_size inherited from the canvas
    #[allow(dead_code)]
    pub fn label_centered(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        text: Arc<str>,
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: radius,
            text,
            fg_color: self.fg_color,
            size: self.font_size,
            on_render_fg: Some(Control::render_text_centered),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a sprite. Will not draw anything until set_sprite is called.
    pub fn sprite(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: 0.,
            on_render_bg: Some(Control::render_sprite_bg),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a sprite that highlights on pointer hover. Will not draw anything until set_sprite is called.
    #[allow(dead_code)]
    pub fn sprite_interactive(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: 0.,
            on_render_bg: Some(Control::render_sprite_bg),
            on_render_hl: Some(Control::render_sprite_hl),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a button with fg_color, bg_color, font_size inherited from the canvas
    pub fn button(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        text: Arc<str>,
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();

        self.canvas.interactive_set_idx(x, y, w, h, idx);
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: radius,
            text,
            fg_color: self.fg_color,
            bg_color: self.bg_color,
            size: self.font_size,
            on_render_bg: Some(Control::render_rounded_rect),
            on_render_fg: Some(Control::render_text_centered),
            on_render_hl: Some(Control::render_highlight),
            ..Control::new()
        });

        &mut self.canvas.controls[idx]
    }

    #[allow(clippy::too_many_arguments)]
    pub fn key_button(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        cap_type: KeyCapType,
        label: &[String],
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.interactive_set_idx(x, y, w, h, idx);

        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            corner_radius: radius,
            bg_color: self.bg_color,
            on_render_bg: Some(Control::render_rounded_rect),
            on_render_hl: Some(Control::render_highlight),
            ..Control::new()
        });

        let renders = match cap_type {
            KeyCapType::Regular => {
                let render: ControlRenderer<D, S> = Control::render_text_centered;
                let rect = Rect {
                    x,
                    y,
                    w,
                    h: h - self.font_size as f32,
                };
                vec![(render, rect, 1f32)]
            }
            KeyCapType::RegularAltGr => {
                let render: ControlRenderer<D, S> = Control::render_text;
                let rect0 = Rect {
                    x: x + 12.,
                    y: y + (self.font_size as f32) + 12.,
                    w,
                    h,
                };
                let rect1 = Rect {
                    x: w.mul_add(0.5, x) + 12.,
                    y: y + h - (self.font_size as f32) + 8.,
                    w,
                    h,
                };
                vec![(render, rect0, 1.0), (render, rect1, 0.8)]
            }
            KeyCapType::Reversed => {
                let render: ControlRenderer<D, S> = Control::render_text_centered;
                let rect0 = Rect {
                    x,
                    y: y + 2.0,
                    w,
                    h: h * 0.5,
                };
                let rect1 = Rect {
                    x,
                    y: h.mul_add(0.5, y) + 2.0,
                    w,
                    h: h * 0.5,
                };
                vec![(render, rect1, 1.0), (render, rect0, 0.8)]
            }
            KeyCapType::ReversedAltGr => {
                let render: ControlRenderer<D, S> = Control::render_text;
                let rect0 = Rect {
                    x: x + 12.,
                    y: y + (self.font_size as f32) + 8.,
                    w,
                    h,
                };
                let rect1 = Rect {
                    x: x + 12.,
                    y: y + h - (self.font_size as f32) + 4.,
                    w,
                    h,
                };
                let rect2 = Rect {
                    x: w.mul_add(0.5, x) + 8.,
                    y: y + h - (self.font_size as f32) + 4.,
                    w,
                    h,
                };
                vec![
                    (render, rect1, 1.0),
                    (render, rect0, 0.8),
                    (render, rect2, 0.8),
                ]
            }
        };

        for (idx, (render, rect, alpha)) in renders.into_iter().enumerate() {
            if idx >= label.len() {
                break;
            }

            self.canvas.controls.push(Control {
                rect,
                text: Arc::from(label[idx].as_str()),
                fg_color: self.fg_color * alpha,
                size: self.font_size,
                on_render_fg: Some(render),
                ..Control::new()
            });
        }

        &mut self.canvas.controls[idx]
    }
}
