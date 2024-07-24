use glam::Vec4;
use std::{sync::Arc, f32::consts::PI};
use vulkano::{
    buffer::{Buffer, BufferUsage, BufferCreateInfo},
    image::view::ImageView,
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter}};

use crate::{
    backend::input::PointerMode, graphics::{WlxCommandBuffer, Vert2Uv}, gui::GuiColor, state::AppState,
};

use super::{CanvasData, Rect};

pub type ControlRenderer<D, S> =
    fn(&Control<D, S>, &CanvasData<D>, &mut AppState, &mut WlxCommandBuffer) -> anyhow::Result<()>;

pub type ControlRendererHl<D, S> = fn(
    &Control<D, S>,
    &CanvasData<D>,
    &mut AppState,
    &mut WlxCommandBuffer,
    Vec4,
) -> anyhow::Result<()>;

pub(crate) struct Control<D, S> {
    pub state: Option<S>,
    pub rect: Rect,
    pub corner_radius: f32,
    pub fg_color: GuiColor,
    pub bg_color: GuiColor,
    pub text: Arc<str>,
    pub size: isize,
    pub sprite: Option<Arc<ImageView>>,
    pub sprite_st: Vec4,
    pub(super) dirty: bool,

    pub on_update: Option<fn(&mut Self, &mut D, &mut AppState)>,
    pub on_press: Option<fn(&mut Self, &mut D, &mut AppState, PointerMode)>,
    pub on_release: Option<fn(&mut Self, &mut D, &mut AppState)>,
    pub on_scroll: Option<fn(&mut Self, &mut D, &mut AppState, f32)>,
    pub test_highlight: Option<fn(&Self, &mut D, &mut AppState) -> Option<Vec4>>,

    pub(super) on_render_bg: Option<ControlRenderer<D, S>>,
    pub(super) on_render_hl: Option<ControlRendererHl<D, S>>,
    pub(super) on_render_fg: Option<ControlRenderer<D, S>>,
}

impl<D, S> Control<D, S> {
    pub(super) fn new() -> Self {
        Self {
            rect: Rect {
                x: 0.,
                y: 0.,
                w: 0.,
                h: 0.,
            },
            corner_radius: 0.,
            fg_color: Vec4::ONE,
            bg_color: Vec4::ZERO,
            text: Arc::from(""),
            sprite: None,
            sprite_st: Vec4::new(1., 1., 0., 0.),
            dirty: true,
            size: 24,
            state: None,
            on_update: None,
            on_render_bg: None,
            on_render_hl: None,
            on_render_fg: None,
            test_highlight: None,
            on_press: None,
            on_release: None,
            on_scroll: None,
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

    #[inline(always)]
    pub fn set_sprite(&mut self, sprite: Arc<ImageView>) {
        self.sprite.replace(sprite);
        self.dirty = true;
    }

    #[inline(always)]
    pub fn set_sprite_st(&mut self, sprite_st: Vec4) {
        if self.sprite_st == sprite_st {
            return;
        }
        self.sprite_st = sprite_st;
        self.dirty = true;
    }

    #[inline(always)]
    pub fn set_fg_color(&mut self, color: GuiColor) {
        if self.fg_color == color {
            return;
        }
        self.fg_color = color;
        self.dirty = true;
    }

    pub fn render_rounded_rect(
        &self,
        canvas: &CanvasData<D>,
        _: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<()> {
        let pass = {
            let r = self.corner_radius.min(self.rect.w / 2.0).min(self.rect.h / 2.0);
            let rw = r / canvas.width as f32;
            let ruw = r / self.rect.w as f32;
            let rh = r / canvas.height as f32;
            let ruh = r / self.rect.h as f32;

            let x0 = self.rect.x / canvas.width as f32 + rw;
            let y0 = self.rect.y / canvas.height as f32 + rh;

            let x1 = self.rect.w / canvas.width as f32 + x0 - rw - rw;
            let y1 = self.rect.h / canvas.height as f32 + y0 - rh - rh;

            let pi6s = (PI/6.).sin();
            let pi6c = (PI/6.).cos();
            let pi3s = (PI/3.).sin();
            let pi3c = (PI/3.).cos();

            let vertices = [
                // Top Left Corner (0-3)
                Vert2Uv {
                    in_pos: [x0 - rw, y0],
                    in_uv: [0.0, ruh],
                },
                Vert2Uv {
                    in_pos: [x0 - rw * pi6c, y0 - rh * pi6s],
                    in_uv: [ruw - ruw * pi6c, ruh - ruh * pi6s],
                },
                Vert2Uv {
                    in_pos: [x0 - rw * pi3c, y0 - rh * pi3s],
                    in_uv: [ruw - ruw * pi3c, ruh - ruh * pi3s],
                },
                Vert2Uv {
                    in_pos: [x0, y0 - rh],
                    in_uv: [ruw, 0.0],
                },

                // Top Right Corner (4-7)
                Vert2Uv {
                    in_pos: [x1, y0 - rh],
                    in_uv: [1.0 - ruw, 0.0],
                },
                Vert2Uv {
                    in_pos: [x1 + rw * pi3c, y0 - rh * pi3s],
                    in_uv: [1.0 - ruw + ruw * pi3c, ruh - ruh * pi3s],
                },
                Vert2Uv {
                    in_pos: [x1 + rw * pi6c, y0 - rh * pi6s],
                    in_uv: [1.0 - ruw + ruw * pi6c, ruh - ruh * pi6s],
                },
                Vert2Uv {
                    in_pos: [x1 + rw, y0],
                    in_uv: [1.0, ruh],
                },

                // Bottom Right Corner (8-11)
                Vert2Uv {
                    in_pos: [x1 + rw, y1],
                    in_uv: [1.0, 1.0 - ruh],
                },
                Vert2Uv {
                    in_pos: [x1 + rw * pi6c, y1 + rh * pi6s],
                    in_uv: [1.0 - ruw + ruw * pi6c, 1.0 - ruh + ruh * pi6s],
                },
                Vert2Uv {
                    in_pos: [x1 + rw * pi3c, y1 + rh * pi3s],
                    in_uv: [1.0 - ruw + ruw * pi3c, 1.0 - ruh + ruh * pi3s],
                },
                Vert2Uv {
                    in_pos: [x1, y1 + rh],
                    in_uv: [1.0 - ruw, 1.0],
                },

                // Bottom Left Corner (12-15)
                Vert2Uv {
                    in_pos: [x0, y1 + rh],
                    in_uv: [ruw, 1.0],
                },
                Vert2Uv {
                    in_pos: [x0 - rw * pi3c, y1 + rh * pi3s],
                    in_uv: [ruw - ruw * pi3c, 1.0 - ruh + ruh * pi3s],
                },
                Vert2Uv {
                    in_pos: [x0 - rw * pi6c, y1 + rh * pi6s],
                    in_uv: [ruw - ruw * pi6c, 1.0 - ruh + ruh * pi6s],
                },
                Vert2Uv {
                    in_pos: [x0 - rw, y1],
                    in_uv: [0.0, 1.0 - ruh],
                },
            ];

            let mut vertex_string = String::from("[");
            for vertex in vertices.iter() {
                vertex_string.push('(');
                vertex_string.push_str(&vertex.in_uv[0].to_string());
                vertex_string.push(',');
                vertex_string.push_str(&vertex.in_uv[1].to_string());
                vertex_string.push_str("),");
            }
            vertex_string.push(']');
            //log::info!("{}", vertex_string);

            let vertex_buffer = canvas.graphics.upload_buffer(BufferUsage::VERTEX_BUFFER, vertices.iter())?;

            let set0 = canvas
                .pipeline_bg_color
                .uniform_buffer(0, self.bg_color.to_array().to_vec())?;

            let indices: [u16; 42] =
                [0,1,15, 14,15,1
                ,1,2,14, 13,14,2
                ,2,3,13, 12,13,3
                ,3,4,12, 11,12,4
                ,4,5,11, 10,12,5
                ,5,6,10,  9,10,6
                ,6,7,9,   8, 9,7];

            canvas.pipeline_bg_color.create_pass(
                [canvas.width as _, canvas.height as _],
                vertex_buffer,
                Buffer::from_iter(
                    canvas.graphics.memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::INDEX_BUFFER,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    indices.iter().cloned(),
                )?,
                vec![set0],
            )?
        };

        cmd_buffer.run_ref(&pass)
    }

    pub(super) fn render_rect(
        &self,
        canvas: &CanvasData<D>,
        _: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<()> {
        let pass = {
            let vertex_buffer = canvas.graphics.upload_verts(
                canvas.width as _,
                canvas.height as _,
                self.rect.x,
                self.rect.y,
                self.rect.w,
                self.rect.h,
            )?;
            let set0 = canvas
                .pipeline_bg_color
                .uniform_buffer(0, self.bg_color.to_array().to_vec())?;
            canvas.pipeline_bg_color.create_pass(
                [canvas.width as _, canvas.height as _],
                vertex_buffer,
                canvas.graphics.quad_indices.clone(),
                vec![set0],
            )?
        };

        cmd_buffer.run_ref(&pass)
    }

    pub(super) fn render_highlight(
        &self,
        canvas: &CanvasData<D>,
        _: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
        color: GuiColor,
    ) -> anyhow::Result<()> {
        let vertex_buffer = canvas.graphics.upload_verts(
            canvas.width as _,
            canvas.height as _,
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
        )?;

        let set0 = canvas
            .pipeline_bg_color
            .uniform_buffer(0, color.to_array().to_vec())?;

        let pass = canvas.pipeline_bg_color.create_pass(
            [canvas.width as _, canvas.height as _],
            vertex_buffer.clone(),
            canvas.graphics.quad_indices.clone(),
            vec![set0],
        )?;

        cmd_buffer.run_ref(&pass)
    }

    pub(super) fn render_text(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<()> {
        let mut cur_y = self.rect.y;
        for line in self.text.lines() {
            let mut cur_x = self.rect.x;
            for glyph in app
                .fc
                .get_glyphs(line, self.size, canvas.graphics.clone())?
            {
                if let Some(tex) = glyph.tex.clone() {
                    let vertex_buffer = canvas.graphics.upload_verts(
                        canvas.width as _,
                        canvas.height as _,
                        cur_x + glyph.left,
                        cur_y - glyph.top,
                        glyph.width,
                        glyph.height,
                    )?;
                    let set0 = canvas.pipeline_fg_glyph.uniform_sampler(
                        0,
                        ImageView::new_default(tex)?,
                        app.graphics.texture_filtering,
                    )?;
                    let set1 = canvas
                        .pipeline_fg_glyph
                        .uniform_buffer(1, self.fg_color.to_array().to_vec())?;
                    let pass = canvas.pipeline_fg_glyph.create_pass(
                        [canvas.width as _, canvas.height as _],
                        vertex_buffer,
                        canvas.graphics.quad_indices.clone(),
                        vec![set0, set1],
                    )?;
                    cmd_buffer.run_ref(&pass)?;
                }
                cur_x += glyph.advance;
            }
            cur_y += (self.size as f32) * 1.5;
        }
        Ok(())
    }

    pub(super) fn render_text_centered(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<()> {
        let (w, h) = app
            .fc
            .get_text_size(&self.text, self.size, canvas.graphics.clone())?;

        let mut cur_y = self.rect.y + (self.rect.h) - (h * 0.5) - (self.size as f32 * 0.25);
        for line in self.text.lines() {
            let mut cur_x = self.rect.x + (self.rect.w * 0.5) - (w * 0.5);
            for glyph in app
                .fc
                .get_glyphs(line, self.size, canvas.graphics.clone())?
            {
                if let Some(tex) = glyph.tex.clone() {
                    let vertex_buffer = canvas.graphics.upload_verts(
                        canvas.width as _,
                        canvas.height as _,
                        cur_x + glyph.left,
                        cur_y - glyph.top,
                        glyph.width,
                        glyph.height,
                    )?;
                    let set0 = canvas.pipeline_fg_glyph.uniform_sampler(
                        0,
                        ImageView::new_default(tex)?,
                        app.graphics.texture_filtering,
                    )?;
                    let set1 = canvas
                        .pipeline_fg_glyph
                        .uniform_buffer(1, self.fg_color.to_array().to_vec())?;
                    let pass = canvas.pipeline_fg_glyph.create_pass(
                        [canvas.width as _, canvas.height as _],
                        vertex_buffer,
                        canvas.graphics.quad_indices.clone(),
                        vec![set0, set1],
                    )?;
                    cmd_buffer.run_ref(&pass)?;
                }
                cur_x += glyph.advance;
            }
            cur_y += (self.size as f32) * 1.5;
        }
        Ok(())
    }

    pub(super) fn render_sprite_bg(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
    ) -> anyhow::Result<()> {
        let Some(view) = self.sprite.as_ref() else {
            return Ok(());
        };

        let vertex_buffer = canvas.graphics.upload_verts(
            canvas.width as _,
            canvas.height as _,
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
        )?;
        let set0 = canvas.pipeline_bg_sprite.uniform_sampler(
            0,
            view.clone(),
            app.graphics.texture_filtering,
        )?;

        let uniform = vec![
            self.sprite_st.x,
            self.sprite_st.y,
            self.sprite_st.z,
            self.sprite_st.w,
            self.fg_color.x,
            self.fg_color.y,
            self.fg_color.z,
            self.fg_color.w,
        ];

        let set1 = canvas.pipeline_bg_sprite.uniform_buffer(1, uniform)?;

        let pass = canvas.pipeline_bg_sprite.create_pass(
            [canvas.width as _, canvas.height as _],
            vertex_buffer,
            canvas.graphics.quad_indices.clone(),
            vec![set0, set1],
        )?;
        cmd_buffer.run_ref(&pass)?;
        Ok(())
    }

    pub(super) fn render_sprite_hl(
        &self,
        canvas: &CanvasData<D>,
        app: &mut AppState,
        cmd_buffer: &mut WlxCommandBuffer,
        color: GuiColor,
    ) -> anyhow::Result<()> {
        let Some(view) = self.sprite.as_ref() else {
            return Ok(());
        };

        let vertex_buffer = canvas.graphics.upload_verts(
            canvas.width as _,
            canvas.height as _,
            self.rect.x,
            self.rect.y,
            self.rect.w,
            self.rect.h,
        )?;
        let set0 = canvas.pipeline_hl_sprite.uniform_sampler(
            0,
            view.clone(),
            app.graphics.texture_filtering,
        )?;

        let uniform = vec![
            self.sprite_st.x,
            self.sprite_st.y,
            self.sprite_st.z,
            self.sprite_st.w,
            color.x,
            color.y,
            color.z,
            color.w,
        ];

        let set1 = canvas.pipeline_hl_sprite.uniform_buffer(1, uniform)?;

        let pass = canvas.pipeline_hl_sprite.create_pass(
            [canvas.width as _, canvas.height as _],
            vertex_buffer,
            canvas.graphics.quad_indices.clone(),
            vec![set0, set1],
        )?;
        cmd_buffer.run_ref(&pass)?;
        Ok(())
    }
}
