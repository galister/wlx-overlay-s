pub(crate) mod builder;
pub(crate) mod control;

use std::sync::Arc;

use glam::{Vec2, Vec4};
use vulkano::{
    command_buffer::CommandBufferUsage,
    format::Format,
    image::{view::ImageView, ImageLayout},
};

use crate::{
    backend::{
        input::{Haptics, InteractionHandler, PointerHit},
        overlay::{OverlayBackend, OverlayRenderer},
    },
    graphics::{WlxGraphics, WlxPass, WlxPipeline, WlxPipelineLegacy, BLEND_ALPHA},
    state::AppState,
};

const RES_DIVIDER: usize = 4;

pub struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
}

pub struct CanvasData<D> {
    pub data: D,
    pub width: usize,
    pub height: usize,

    graphics: Arc<WlxGraphics>,

    pipeline_bg_color: Arc<WlxPipeline<WlxPipelineLegacy>>,
    pipeline_fg_glyph: Arc<WlxPipeline<WlxPipelineLegacy>>,
    pipeline_bg_sprite: Arc<WlxPipeline<WlxPipelineLegacy>>,
    pipeline_hl_sprite: Arc<WlxPipeline<WlxPipelineLegacy>>,
    pipeline_final: Arc<WlxPipeline<WlxPipelineLegacy>>,
}

pub struct Canvas<D, S> {
    controls: Vec<control::Control<D, S>>,
    canvas: CanvasData<D>,

    hover_controls: [Option<usize>; 2],
    pressed_controls: [Option<usize>; 2],

    interact_map: Vec<Option<u16>>,
    interact_stride: usize,
    interact_rows: usize,

    view_final: Arc<ImageView>,

    pass_fg: WlxPass<WlxPipelineLegacy>,
    pass_bg: WlxPass<WlxPipelineLegacy>,
}

impl<D, S> Canvas<D, S> {
    fn new(
        width: usize,
        height: usize,
        graphics: Arc<WlxGraphics>,
        format: Format,
        data: D,
    ) -> anyhow::Result<Self> {
        let tex_fg = graphics.render_texture(width as _, height as _, format)?;
        let tex_bg = graphics.render_texture(width as _, height as _, format)?;
        let tex_final = graphics.render_texture(width as _, height as _, format)?;

        let view_fg = ImageView::new_default(tex_fg.clone())?;
        let view_bg = ImageView::new_default(tex_bg.clone())?;
        let view_final = ImageView::new_default(tex_final.clone())?;

        let Ok(shaders) = graphics.shared_shaders.read() else {
            anyhow::bail!("Failed to lock shared shaders for reading");
        };

        let pipeline_bg_color = graphics.create_pipeline(
            view_bg.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_color").unwrap().clone(),  // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_fg_glyph = graphics.create_pipeline(
            view_fg.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_glyph").unwrap().clone(),  // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_bg_sprite = graphics.create_pipeline(
            view_fg.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_sprite2").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_hl_sprite = graphics.create_pipeline(
            view_fg.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_sprite2_hl").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let vertex_buffer =
            graphics.upload_verts(width as _, height as _, 0., 0., width as _, height as _)?;

        let pipeline_final = graphics.create_pipeline_with_layouts(
            view_final.clone(),
            shaders.get("vert_common").unwrap().clone(), // want panic
            shaders.get("frag_sprite").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
            ImageLayout::TransferSrcOptimal,
            ImageLayout::TransferSrcOptimal,
        )?;

        let set_fg =
            pipeline_final.uniform_sampler(0, view_fg.clone(), graphics.texture_filtering)?;
        let set_bg =
            pipeline_final.uniform_sampler(0, view_bg.clone(), graphics.texture_filtering)?;
        let pass_fg = pipeline_final.create_pass(
            [width as _, height as _],
            vertex_buffer.clone(),
            graphics.quad_indices.clone(),
            vec![set_fg],
        )?;
        let pass_bg = pipeline_final.create_pass(
            [width as _, height as _],
            vertex_buffer.clone(),
            graphics.quad_indices.clone(),
            vec![set_bg],
        )?;

        let stride = width / RES_DIVIDER;
        let rows = height / RES_DIVIDER;

        Ok(Self {
            canvas: CanvasData {
                data,
                width,
                height,
                graphics: graphics.clone(),
                pipeline_bg_color,
                pipeline_fg_glyph,
                pipeline_bg_sprite,
                pipeline_hl_sprite,
                pipeline_final,
            },
            controls: Vec::new(),
            hover_controls: [None, None],
            pressed_controls: [None, None],
            interact_map: vec![None; stride * rows],
            interact_stride: stride,
            interact_rows: rows,
            view_final,
            pass_fg,
            pass_bg,
        })
    }

    fn interactive_set_idx(&mut self, x: f32, y: f32, w: f32, h: f32, idx: usize) {
        let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);

        let x_min = (x / RES_DIVIDER).max(0);
        let y_min = (y / RES_DIVIDER).max(0);
        let x_max = (x_min + (w / RES_DIVIDER)).min(self.interact_stride - 1);
        let y_max = (y_min + (h / RES_DIVIDER)).min(self.interact_rows - 1);

        for y in y_min..y_max {
            for x in x_min..x_max {
                self.interact_map[y * self.interact_stride + x] = Some(idx as u16);
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

    fn render_bg(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_render_pass(&self.canvas.pipeline_bg_color)?;
        for c in self.controls.iter_mut() {
            if let Some(fun) = c.on_render_bg {
                fun(c, &self.canvas, app, &mut cmd_buffer)?;
            }
        }
        cmd_buffer.end_render_pass()?;
        cmd_buffer.build_and_execute_now()
    }

    fn render_fg(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_render_pass(&self.canvas.pipeline_fg_glyph)?;
        for c in self.controls.iter_mut() {
            if let Some(fun) = c.on_render_fg {
                fun(c, &self.canvas, app, &mut cmd_buffer)?;
            }
        }
        cmd_buffer.end_render_pass()?;
        cmd_buffer.build_and_execute_now()
    }
}

impl<D, S> InteractionHandler for Canvas<D, S> {
    fn on_left(&mut self, _app: &mut AppState, pointer: usize) {
        self.hover_controls[pointer] = None;
    }
    fn on_hover(&mut self, _app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        let old = self.hover_controls[hit.pointer];
        if let Some(i) = self.interactive_get_idx(hit.uv) {
            self.hover_controls[hit.pointer] = Some(i);
        } else {
            self.hover_controls[hit.pointer] = None;
        }
        if old != self.hover_controls[hit.pointer] {
            Some(Haptics {
                intensity: 0.1,
                duration: 0.01,
                frequency: 5.0,
            })
        } else {
            None
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
                    f(c, &mut self.canvas.data, app, hit.mode);
                }
            } else if let Some(ref mut f) = c.on_release {
                self.pressed_controls[hit.pointer] = None;
                f(c, &mut self.canvas.data, app);
            }
        }
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta: f32) {
        let idx = self.hover_controls[hit.pointer];

        if let Some(idx) = idx {
            let c = &mut self.controls[idx];
            if let Some(ref mut f) = c.on_scroll {
                f(c, &mut self.canvas.data, app, delta);
            }
        }
    }
}

impl<D, S> OverlayRenderer for Canvas<D, S> {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.render_bg(app)?;
        self.render_fg(app)
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn render(&mut self, app: &mut AppState) -> anyhow::Result<()> {
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

        if dirty {
            self.render_bg(app)?;
            self.render_fg(app)?;
        }

        /*
        let image = self.view_final.image().clone();
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
        */

        let mut cmd_buffer = self
            .canvas
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_render_pass(&self.canvas.pipeline_final)?;

        // static background
        cmd_buffer.run_ref(&self.pass_bg)?;

        for (i, c) in self.controls.iter_mut().enumerate() {
            if let Some(render) = c.on_render_hl {
                if let Some(test) = c.test_highlight {
                    if let Some(hl_color) = test(c, &mut self.canvas.data, app) {
                        render(c, &self.canvas, app, &mut cmd_buffer, hl_color)?;
                    }
                }
                if self.hover_controls.contains(&Some(i)) {
                    render(
                        c,
                        &self.canvas,
                        app,
                        &mut cmd_buffer,
                        Vec4::new(1., 1., 1., 0.3),
                    )?;
                }
            }
        }

        // mostly static text
        cmd_buffer.run_ref(&self.pass_fg)?;

        cmd_buffer.end_render_pass()?;
        cmd_buffer.build_and_execute_now()

        /*
        self.canvas
            .graphics
            .transition_layout(
                image,
                ImageLayout::ColorAttachmentOptimal,
                ImageLayout::TransferSrcOptimal,
            )
            .wait(None)
            .unwrap();
        */
    }
    fn view(&mut self) -> Option<Arc<ImageView>> {
        Some(self.view_final.clone())
    }
}

impl<D, S> OverlayBackend for Canvas<D, S> {
    fn set_renderer(&mut self, _renderer: Box<dyn OverlayRenderer>) {}
    fn set_interaction(&mut self, _interaction: Box<dyn InteractionHandler>) {}
}
