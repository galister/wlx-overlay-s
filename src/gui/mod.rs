use std::sync::Arc;

use glam::{Vec2, Vec3};
use vulkano::{
    command_buffer::{CommandBufferUsage, PrimaryAutoCommandBuffer},
    format::Format,
    image::{view::ImageView, AttachmentImage, ImageAccess, ImageLayout, ImageViewAbstract},
    sampler::Filter,
};

use crate::{
    backend::{
        input::{InteractionHandler, PointerHit},
        overlay::{OverlayBackend, OverlayRenderer},
    },
    graphics::{WlxCommandBuffer, WlxGraphics, WlxPass, WlxPipeline},
    shaders::{frag_color, frag_glyph, frag_sprite, vert_common},
    state::AppState,
};

pub mod font;

const RES_DIVIDER: usize = 4;

struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

// Parses a color from a HTML hex string
pub fn color_parse(html_hex: &str) -> Vec3 {
    let mut color = Vec3::ZERO;
    color.x = u8::from_str_radix(&html_hex[1..3], 16).unwrap() as f32 / 255.;
    color.y = u8::from_str_radix(&html_hex[3..5], 16).unwrap() as f32 / 255.;
    color.z = u8::from_str_radix(&html_hex[5..7], 16).unwrap() as f32 / 255.;
    color
}

pub struct CanvasBuilder<D, S> {
    canvas: Canvas<D, S>,

    pub fg_color: Vec3,
    pub bg_color: Vec3,
    pub font_size: isize,
}

impl<D, S> CanvasBuilder<D, S> {
    pub fn new(
        width: usize,
        height: usize,
        graphics: Arc<WlxGraphics>,
        format: Format,
        data: D,
    ) -> Self {
        Self {
            canvas: Canvas::new(width, height, graphics, format, data),
            bg_color: Vec3::ZERO,
            fg_color: Vec3::ONE,
            font_size: 16,
        }
    }

    pub fn build(self) -> Canvas<D, S> {
        self.canvas
    }

    // Creates a panel with bg_color inherited from the canvas
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            bg_color: self.bg_color,
            on_render_bg: Some(Control::render_rect),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a label with fg_color, font_size inherited from the canvas
    pub fn label(&mut self, x: f32, y: f32, w: f32, h: f32, text: Arc<str>) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
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
        text: Arc<str>,
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            text,
            fg_color: self.fg_color,
            size: self.font_size,
            on_render_fg: Some(Control::render_text_centered),
            ..Control::new()
        });
        &mut self.canvas.controls[idx]
    }

    // Creates a button with fg_color, bg_color, font_size inherited from the canvas
    pub fn button(&mut self, x: f32, y: f32, w: f32, h: f32, text: Arc<str>) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();

        self.canvas.interactive_set_idx(x, y, w, h, idx);
        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            text,
            fg_color: self.fg_color,
            bg_color: self.bg_color,
            size: self.font_size,
            on_render_bg: Some(Control::render_rect),
            on_render_fg: Some(Control::render_text_centered),
            on_render_hl: Some(Control::render_highlight),
            ..Control::new()
        });

        &mut self.canvas.controls[idx]
    }

    pub fn key_button(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &Vec<String>,
    ) -> &mut Control<D, S> {
        let idx = self.canvas.controls.len();
        self.canvas.interactive_set_idx(x, y, w, h, idx);

        self.canvas.controls.push(Control {
            rect: Rect { x, y, w, h },
            bg_color: self.bg_color,
            on_render_bg: Some(Control::render_rect),
            on_render_hl: Some(Control::render_highlight),
            ..Control::new()
        });

        for (i, item) in label.iter().enumerate().take(label.len().min(2)) {
            self.canvas.controls.push(Control {
                rect: if i == 0 {
                    Rect {
                        x: x + 4.,
                        y: y + (self.font_size as f32) + 4.,
                        w,
                        h,
                    }
                } else {
                    Rect {
                        x: x + w * 0.5,
                        y: y + h - (self.font_size as f32) + 4.,
                        w,
                        h,
                    }
                },
                text: Arc::from(item.as_str()),
                fg_color: self.fg_color,
                size: self.font_size,
                on_render_fg: Some(Control::render_text),
                ..Control::new()
            });
        }

        &mut self.canvas.controls[idx]
    }
}

pub struct CanvasData<D> {
    pub data: D,
    pub width: usize,
    pub height: usize,

    graphics: Arc<WlxGraphics>,

    pipeline_color: Arc<WlxPipeline>,
    pipeline_glyph: Arc<WlxPipeline>,
}

pub struct Canvas<D, S> {
    controls: Vec<Control<D, S>>,
    canvas: CanvasData<D>,

    hover_controls: [Option<usize>; 2],
    pressed_controls: [Option<usize>; 2],

    interact_map: Vec<Option<u8>>,
    interact_stride: usize,
    interact_rows: usize,

    view_fg: Arc<ImageView<AttachmentImage>>,
    view_bg: Arc<ImageView<AttachmentImage>>,
    view_final: Arc<ImageView<AttachmentImage>>,

    pass_fg: WlxPass,
    pass_bg: WlxPass,

    first_render: bool,
}

impl<D, S> Canvas<D, S> {
    fn new(
        width: usize,
        height: usize,
        graphics: Arc<WlxGraphics>,
        format: Format,
        data: D,
    ) -> Self {
        let pipeline_color = graphics.create_pipeline(
            vert_common::load(graphics.device.clone()).unwrap(),
            frag_color::load(graphics.device.clone()).unwrap(),
            format,
        );

        let pipeline_glyph = graphics.create_pipeline(
            vert_common::load(graphics.device.clone()).unwrap(),
            frag_glyph::load(graphics.device.clone()).unwrap(),
            format,
        );

        let vertex_buffer =
            graphics.upload_verts(width as _, height as _, 0., 0., width as _, height as _);

        let pipeline = graphics.create_pipeline(
            vert_common::load(graphics.device.clone()).unwrap(),
            frag_sprite::load(graphics.device.clone()).unwrap(),
            format,
        );

        let tex_fg = graphics.render_texture(width as _, height as _, format);
        let tex_bg = graphics.render_texture(width as _, height as _, format);
        let tex_final = graphics.render_texture(width as _, height as _, format);

        let view_fg = ImageView::new_default(tex_fg.clone()).unwrap();
        let view_bg = ImageView::new_default(tex_bg.clone()).unwrap();
        let view_final = ImageView::new_default(tex_final.clone()).unwrap();

        let set_fg = pipeline.uniform_sampler(0, view_fg.clone(), Filter::Nearest);
        let set_bg = pipeline.uniform_sampler(0, view_bg.clone(), Filter::Nearest);
        let pass_fg = pipeline.create_pass(
            [width as _, height as _],
            vertex_buffer.clone(),
            graphics.quad_indices.clone(),
            vec![set_fg],
        );
        let pass_bg = pipeline.create_pass(
            [width as _, height as _],
            vertex_buffer.clone(),
            graphics.quad_indices.clone(),
            vec![set_bg],
        );

        let stride = width / RES_DIVIDER;
        let rows = height / RES_DIVIDER;

        Self {
            canvas: CanvasData {
                data,
                width,
                height,
                graphics,
                pipeline_color,
                pipeline_glyph,
            },
            controls: Vec::new(),
            hover_controls: [None, None],
            pressed_controls: [None, None],
            interact_map: vec![None; stride * rows],
            interact_stride: stride,
            interact_rows: rows,
            view_fg,
            view_bg,
            view_final,
            pass_fg,
            pass_bg,
            first_render: true,
        }
    }

    fn interactive_set_idx(&mut self, x: f32, y: f32, w: f32, h: f32, idx: usize) {
        let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);

        let x_min = (x / RES_DIVIDER).max(0);
        let y_min = (y / RES_DIVIDER).max(0);
        let x_max = (x_min + (w / RES_DIVIDER)).min(self.interact_stride - 1);
        let y_max = (y_min + (h / RES_DIVIDER)).min(self.interact_rows - 1);

        for y in y_min..y_max {
            for x in x_min..x_max {
                self.interact_map[y * self.interact_stride + x] = Some(idx as u8);
            }
        }
    }

    fn interactive_get_idx(&self, uv: Vec2) -> Option<usize> {
        let x = (uv.x * self.canvas.width as f32) as usize;
        let y = (uv.y * self.canvas.height as f32) as usize;
        let x = (x / RES_DIVIDER).max(0).min(self.interact_stride - 1);
        let y = (y / RES_DIVIDER).max(0).min(self.interact_rows - 1);
        self.interact_map[y * self.interact_stride + x].map(|x| x as usize)
    }

    fn render_bg(&mut self, app: &mut AppState) {
        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin(self.view_bg.clone());
        for c in self.controls.iter_mut() {
            if let Some(fun) = c.on_render_bg {
                fun(c, &self.canvas, app, &mut cmd_buffer);
            }
        }
        cmd_buffer.end_render().build_and_execute_now()
    }

    fn render_fg(&mut self, app: &mut AppState) {
        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin(self.view_fg.clone());
        for c in self.controls.iter_mut() {
            if let Some(fun) = c.on_render_fg {
                fun(c, &self.canvas, app, &mut cmd_buffer);
            }
        }
        cmd_buffer.end_render().build_and_execute_now()
    }
}

impl<D, S> InteractionHandler for Canvas<D, S> {
    fn on_left(&mut self, _app: &mut AppState, pointer: usize) {
        self.hover_controls[pointer] = None;
    }
    fn on_hover(&mut self, _app: &mut AppState, hit: &PointerHit) {
        if let Some(i) = self.interactive_get_idx(hit.uv) {
            self.hover_controls[hit.pointer] = Some(i);
        } else {
            self.hover_controls[hit.pointer] = None;
        }
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let idx = if pressed {
            self.interactive_get_idx(hit.uv)
        } else {
            self.pressed_controls[hit.pointer]
        };

        if let Some(idx) = idx {
            let c = &mut self.controls[idx];
            if pressed {
                if let Some(ref mut f) = c.on_press {
                    self.pressed_controls[hit.pointer] = Some(idx);
                    f(c, &mut self.canvas.data, app);
                }
            } else if let Some(ref mut f) = c.on_release {
                self.pressed_controls[hit.pointer] = None;
                f(c, &mut self.canvas.data, app);
            }
        }
    }
    fn on_scroll(&mut self, _app: &mut AppState, _hit: &PointerHit, _delta: f32) {}
}

impl<D, S> OverlayRenderer for Canvas<D, S> {
    fn init(&mut self, app: &mut AppState) {
        self.render_bg(app);
        self.render_fg(app);
    }
    fn pause(&mut self, _app: &mut AppState) {}
    fn resume(&mut self, _app: &mut AppState) {}
    fn render(&mut self, app: &mut AppState) {
        let mut dirty = false;

        for c in self.controls.iter_mut() {
            if let Some(fun) = c.on_update {
                fun(c, &mut self.canvas.data, app);
            }
            if c.dirty {
                dirty = true;
                c.dirty = false;
            }
        }

        let image = self.view_final.image().inner().image.clone();

        if self.first_render {
            self.first_render = false;
        } else {
            self.canvas
                .graphics
                .transition_layout(
                    image.clone(),
                    ImageLayout::TransferSrcOptimal,
                    ImageLayout::ColorAttachmentOptimal,
                )
                .wait(None)
                .unwrap();
        }

        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)
            .begin(self.view_final.clone());

        if dirty {
            self.render_fg(app);
        }

        // static background
        cmd_buffer.run_ref(&self.pass_bg);

        for (i, c) in self.controls.iter_mut().enumerate() {
            if let Some(render) = c.on_render_hl {
                if let Some(test) = c.test_highlight {
                    if test(c, &mut self.canvas.data, app) {
                        render(c, &self.canvas, app, &mut cmd_buffer, true);
                    }
                }
                if self.hover_controls.contains(&Some(i)) {
                    render(c, &self.canvas, app, &mut cmd_buffer, false);
                }
            }
        }

        // mostly static text
        cmd_buffer.run_ref(&self.pass_fg);
        {
            let _ = cmd_buffer.end_render().build_and_execute();
        }
        self.canvas
            .graphics
            .transition_layout(
                image,
                ImageLayout::ColorAttachmentOptimal,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();
    }
    fn view(&mut self) -> Option<Arc<dyn ImageViewAbstract>> {
        Some(self.view_final.clone())
    }
}

impl<D, S> OverlayBackend for Canvas<D, S> {}

pub struct Control<D, S> {
    pub state: Option<S>,
    rect: Rect,
    fg_color: Vec3,
    bg_color: Vec3,
    text: Arc<str>,
    size: isize,
    dirty: bool,

    pub on_update: Option<fn(&mut Self, &mut D, &mut AppState)>,
    pub on_press: Option<fn(&mut Self, &mut D, &mut AppState)>,
    pub on_release: Option<fn(&mut Self, &mut D, &mut AppState)>,
    pub test_highlight: Option<fn(&Self, &mut D, &mut AppState) -> bool>,

    on_render_bg: Option<
        fn(&Self, &CanvasData<D>, &mut AppState, &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>),
    >,
    on_render_hl: Option<
        fn(
            &Self,
            &CanvasData<D>,
            &mut AppState,
            &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>,
            bool,
        ),
    >,
    on_render_fg: Option<
        fn(&Self, &CanvasData<D>, &mut AppState, &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>),
    >,
}

impl<D, S> Control<D, S> {
    fn new() -> Self {
        Self {
            rect: Rect {
                x: 0.,
                y: 0.,
                w: 0.,
                h: 0.,
            },
            fg_color: Vec3::ONE,
            bg_color: Vec3::ZERO,
            text: Arc::from(""),
            dirty: false,
            size: 24,
            state: None,
            on_update: None,
            on_render_bg: None,
            on_render_hl: None,
            on_render_fg: None,
            test_highlight: None,
            on_press: None,
            on_release: None,
        }
    }

    #[inline(always)]
    pub fn set_text(&mut self, text: &str) {
        if *self.text == *text {
            return;
        }
        self.text = text.into();
        self.dirty = true;
    }

    fn render_rect(
        &self,
        canvas: &CanvasData<D>,
        _: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>,
    ) {
        let pass = {
            let vertex_buffer = canvas.graphics.upload_verts(
                canvas.width as _,
                canvas.height as _,
                self.rect.x,
                self.rect.y,
                self.rect.w,
                self.rect.h,
            );
            let set0 = canvas.pipeline_color.uniform_buffer(
                0,
                vec![self.bg_color.x, self.bg_color.y, self.bg_color.z, 1.],
            );
            canvas.pipeline_color.create_pass(
                [canvas.width as _, canvas.height as _],
                vertex_buffer,
                canvas.graphics.quad_indices.clone(),
                vec![set0],
            )
        };

        cmd_buffer.run_ref(&pass);
    }

    fn render_highlight(
        &self,
        canvas: &CanvasData<D>,
        _: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>,
        strong: bool,
    ) {
        let vertex_buffer = canvas.graphics.upload_verts(
            canvas.width as _,
            canvas.height as _,
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
        );
        let set0 = canvas.pipeline_color.uniform_buffer(
            0,
            vec![
                self.bg_color.x,
                self.bg_color.y,
                self.bg_color.z,
                if strong { 0.5 } else { 0.3 },
            ],
        );
        let pass = canvas.pipeline_color.create_pass(
            [canvas.width as _, canvas.height as _],
            vertex_buffer.clone(),
            canvas.graphics.quad_indices.clone(),
            vec![set0],
        );

        cmd_buffer.run_ref(&pass);
    }

    fn render_text(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>,
    ) {
        let mut cur_y = self.rect.y;
        for line in self.text.lines() {
            let mut cur_x = self.rect.x;
            for glyph in app.fc.get_glyphs(line, self.size, canvas.graphics.clone()) {
                if let Some(tex) = glyph.tex.clone() {
                    let vertex_buffer = canvas.graphics.upload_verts(
                        canvas.width as _,
                        canvas.height as _,
                        cur_x + glyph.left,
                        cur_y - glyph.top,
                        glyph.width,
                        glyph.height,
                    );
                    let set0 = canvas.pipeline_glyph.uniform_sampler(
                        0,
                        ImageView::new_default(tex).unwrap(),
                        Filter::Nearest,
                    );
                    let set1 = canvas.pipeline_glyph.uniform_buffer(
                        1,
                        vec![self.fg_color.x, self.fg_color.y, self.fg_color.z, 1.],
                    );
                    let pass = canvas.pipeline_glyph.create_pass(
                        [canvas.width as _, canvas.height as _],
                        vertex_buffer,
                        canvas.graphics.quad_indices.clone(),
                        vec![set0, set1],
                    );
                    cmd_buffer.run_ref(&pass);
                }
                cur_x += glyph.advance;
            }
            cur_y += (self.size as f32) * 1.5;
        }
    }
    fn render_text_centered(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer<PrimaryAutoCommandBuffer>,
    ) {
        let (w, h) = app
            .fc
            .get_text_size(&self.text, self.size, canvas.graphics.clone());

        let mut cur_y = self.rect.y + (self.rect.h) - (h * 0.5);
        for line in self.text.lines() {
            let mut cur_x = self.rect.x + (self.rect.w * 0.5) - (w * 0.5);
            for glyph in app.fc.get_glyphs(line, self.size, canvas.graphics.clone()) {
                if let Some(tex) = glyph.tex.clone() {
                    let vertex_buffer = canvas.graphics.upload_verts(
                        canvas.width as _,
                        canvas.height as _,
                        cur_x + glyph.left,
                        cur_y - glyph.top,
                        glyph.width,
                        glyph.height,
                    );
                    let set0 = canvas.pipeline_glyph.uniform_sampler(
                        0,
                        ImageView::new_default(tex).unwrap(),
                        Filter::Nearest,
                    );
                    let set1 = canvas.pipeline_glyph.uniform_buffer(
                        1,
                        vec![self.fg_color.x, self.fg_color.y, self.fg_color.z, 1.],
                    );
                    let pass = canvas.pipeline_glyph.create_pass(
                        [canvas.width as _, canvas.height as _],
                        vertex_buffer,
                        canvas.graphics.quad_indices.clone(),
                        vec![set0, set1],
                    );
                    cmd_buffer.run_ref(&pass);
                }
                cur_x += glyph.advance;
            }
            cur_y += (self.size as f32) * 1.5;
        }
    }
}
