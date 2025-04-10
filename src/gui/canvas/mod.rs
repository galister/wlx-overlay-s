pub mod builder;
pub mod control;

use std::sync::Arc;

use glam::{Vec2, Vec4};
use vulkano::{command_buffer::CommandBufferUsage, format::Format, image::view::ImageView};

use crate::{
    backend::{
        input::{Haptics, InteractionHandler, PointerHit},
        overlay::{FrameMeta, OverlayBackend, OverlayRenderer, ShouldRender},
    },
    graphics::{CommandBuffers, WlxGraphics, WlxPipeline, BLEND_ALPHA},
    state::AppState,
};

const RES_DIVIDER: usize = 4;

pub struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

pub struct CanvasData<D> {
    pub data: D,
    pub width: usize,
    pub height: usize,

    graphics: Arc<WlxGraphics>,

    pipeline_bg_color: Arc<WlxPipeline>,
    pipeline_bg_sprite: Arc<WlxPipeline>,
    pipeline_fg_glyph: Arc<WlxPipeline>,
    pipeline_hl_color: Arc<WlxPipeline>,
    pipeline_hl_sprite: Arc<WlxPipeline>,
}

pub struct Canvas<D, S> {
    controls: Vec<control::Control<D, S>>,
    data: CanvasData<D>,

    hover_controls: [Option<usize>; 2],
    pressed_controls: [Option<usize>; 2],

    interact_map: Vec<Option<u16>>,
    interact_stride: usize,
    interact_rows: usize,

    pipeline_final: Arc<WlxPipeline>,

    view_fore: Arc<ImageView>,
    view_back: Arc<ImageView>,

    format: Format,

    back_dirty: bool,
    high_dirty: bool,
    fore_dirty: bool,
}

impl<D, S> Canvas<D, S> {
    fn new(
        width: usize,
        height: usize,
        graphics: Arc<WlxGraphics>,
        format: Format,
        data: D,
    ) -> anyhow::Result<Self> {
        let tex_fore = graphics.render_texture(width as _, height as _, format)?;
        let tex_back = graphics.render_texture(width as _, height as _, format)?;

        let view_fore = ImageView::new_default(tex_fore)?;
        let view_back = ImageView::new_default(tex_back)?;

        let Ok(shaders) = graphics.shared_shaders.read() else {
            anyhow::bail!("Failed to lock shared shaders for reading");
        };

        let vert = shaders.get("vert_common").unwrap().clone(); // want panic

        let pipeline_bg_color = graphics.create_pipeline(
            vert.clone(),
            shaders.get("frag_color").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_fg_glyph = graphics.create_pipeline(
            vert.clone(),
            shaders.get("frag_glyph").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_bg_sprite = graphics.create_pipeline(
            vert.clone(),
            shaders.get("frag_sprite2").unwrap().clone(), // want panic
            format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_hl_color = graphics.create_pipeline(
            vert.clone(),
            shaders.get("frag_color").unwrap().clone(), // want panic
            graphics.native_format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_hl_sprite = graphics.create_pipeline(
            vert.clone(),
            shaders.get("frag_sprite2_hl").unwrap().clone(), // want panic
            graphics.native_format,
            Some(BLEND_ALPHA),
        )?;

        let pipeline_final = graphics.create_pipeline(
            vert,
            shaders.get("frag_srgb").unwrap().clone(), // want panic
            graphics.native_format,
            Some(BLEND_ALPHA),
        )?;

        let stride = width / RES_DIVIDER;
        let rows = height / RES_DIVIDER;

        Ok(Self {
            data: CanvasData {
                data,
                width,
                height,
                graphics: graphics.clone(),
                pipeline_bg_color,
                pipeline_bg_sprite,
                pipeline_fg_glyph,
                pipeline_hl_color,
                pipeline_hl_sprite,
            },
            controls: Vec::new(),
            hover_controls: [None, None],
            pressed_controls: [None, None],
            interact_map: vec![None; stride * rows],
            interact_stride: stride,
            interact_rows: rows,
            pipeline_final,
            view_fore,
            view_back,
            format,
            back_dirty: false,
            high_dirty: false,
            fore_dirty: false,
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
        let x = (uv.x * self.data.width as f32) as usize;
        let y = (uv.y * self.data.height as f32) as usize;
        let x = (x / RES_DIVIDER).max(0).min(self.interact_stride - 1);
        let y = (y / RES_DIVIDER).max(0).min(self.interact_rows - 1);
        self.interact_map[y * self.interact_stride + x].map(|x| x as usize)
    }

    pub const fn data_mut(&mut self) -> &mut D {
        &mut self.data.data
    }
}

impl<D, S> InteractionHandler for Canvas<D, S> {
    fn on_left(&mut self, _app: &mut AppState, pointer: usize) {
        self.high_dirty = true;

        self.hover_controls[pointer] = None;
    }
    fn on_hover(&mut self, _app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        // render on every frame if we are being hovered
        self.high_dirty = true;

        let old = self.hover_controls[hit.pointer];
        if let Some(i) = self.interactive_get_idx(hit.uv) {
            self.hover_controls[hit.pointer] = Some(i);
        } else {
            self.hover_controls[hit.pointer] = None;
        }
        if old == self.hover_controls[hit.pointer] {
            None
        } else {
            Some(Haptics {
                intensity: 0.1,
                duration: 0.01,
                frequency: 5.0,
            })
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
                    f(c, &mut self.data.data, app, hit.mode);
                }
            } else if let Some(ref mut f) = c.on_release {
                self.pressed_controls[hit.pointer] = None;
                f(c, &mut self.data.data, app);
            }
        }
    }
    fn on_scroll(&mut self, app: &mut AppState, hit: &PointerHit, delta_y: f32, delta_x: f32) {
        let idx = self.hover_controls[hit.pointer];

        if let Some(idx) = idx {
            let c = &mut self.controls[idx];
            if let Some(ref mut f) = c.on_scroll {
                f(c, &mut self.data.data, app, delta_y, delta_x);
            }
        }
    }
}

impl<D, S> OverlayRenderer for Canvas<D, S> {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        for c in &mut self.controls {
            if let Some(fun) = c.on_update {
                fun(c, &mut self.data.data, app);
            }
            if c.fg_dirty {
                self.fore_dirty = true;
                c.fg_dirty = false;
            }
            if c.bg_dirty {
                self.back_dirty = true;
                c.bg_dirty = false;
            }
        }

        if self.back_dirty || self.fore_dirty || self.high_dirty {
            Ok(ShouldRender::Should)
        } else {
            Ok(ShouldRender::Can)
        }
    }
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        self.high_dirty = false;

        let mut cmd_buffer = self
            .data
            .graphics
            .create_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        if self.back_dirty {
            cmd_buffer.begin_rendering(self.view_back.clone())?;
            for c in &mut self.controls {
                if let Some(fun) = c.on_render_bg {
                    fun(c, &self.data, app, &mut cmd_buffer)?;
                }
            }
            cmd_buffer.end_rendering()?;
            self.back_dirty = false;
        }
        if self.fore_dirty {
            cmd_buffer.begin_rendering(self.view_fore.clone())?;
            for c in &mut self.controls {
                if let Some(fun) = c.on_render_fg {
                    fun(c, &self.data, app, &mut cmd_buffer)?;
                }
            }
            cmd_buffer.end_rendering()?;
            self.fore_dirty = false;
        }

        let set0_fg = self.pipeline_final.uniform_sampler(
            0,
            self.view_fore.clone(),
            app.graphics.texture_filtering,
        )?;
        let set0_bg = self.pipeline_final.uniform_sampler(
            0,
            self.view_back.clone(),
            app.graphics.texture_filtering,
        )?;
        let set1 = self.pipeline_final.uniform_buffer(1, vec![alpha])?;

        let pass_fore = self
            .pipeline_final
            .create_pass_for_target(tgt.clone(), vec![set0_fg, set1.clone()])?;

        let pass_back = self
            .pipeline_final
            .create_pass_for_target(tgt.clone(), vec![set0_bg, set1])?;

        cmd_buffer.begin_rendering(tgt)?;
        cmd_buffer.run_ref(&pass_back)?;

        for (i, c) in self.controls.iter_mut().enumerate() {
            if let Some(render) = c.on_render_hl {
                if let Some(test) = c.test_highlight {
                    if let Some(hl_color) = test(c, &mut self.data.data, app) {
                        render(c, &self.data, app, &mut cmd_buffer, hl_color)?;
                    }
                }
                if self.hover_controls.contains(&Some(i)) {
                    render(
                        c,
                        &self.data,
                        app,
                        &mut cmd_buffer,
                        Vec4::new(1., 1., 1., 0.3),
                    )?;
                }
            }
        }

        // mostly static text
        cmd_buffer.run_ref(&pass_fore)?;

        cmd_buffer.end_rendering()?;
        buf.push(cmd_buffer.build()?);
        Ok(true)
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            extent: [self.data.width as _, self.data.height as _, 1],
            format: self.format,
            ..Default::default()
        })
    }
}

impl<D, S> OverlayBackend for Canvas<D, S> {
    fn set_renderer(&mut self, _renderer: Box<dyn OverlayRenderer>) {}
    fn set_interaction(&mut self, _interaction: Box<dyn InteractionHandler>) {}
}
