use std::{rc::Rc, str::FromStr, sync::Arc};

use fontconfig::{FontConfig, OwnedPattern};
use freetype::{bitmap::PixelMode, face::LoadFlag, Face, Library};
use idmap::IdMap;
use vulkano::{command_buffer::CommandBufferUsage, format::Format, image::Image};

use crate::graphics::WlxGraphics;

const PRIMARY_FONT: &str = "LiberationSans";

pub struct FontCache {
    fc: FontConfig,
    ft: Library,
    collections: IdMap<isize, FontCollection>,
}

struct FontCollection {
    fonts: Vec<Font>,
    cp_map: IdMap<usize, usize>,
}

struct Font {
    face: Face,
    glyphs: IdMap<usize, Rc<Glyph>>,
}

pub struct Glyph {
    pub tex: Option<Arc<Image>>,
    pub top: f32,
    pub left: f32,
    pub width: f32,
    pub height: f32,
    pub advance: f32,
}

impl FontCache {
    pub fn new() -> Self {
        let ft = Library::init().expect("Failed to initialize freetype");
        let fc = FontConfig::default();

        FontCache {
            fc,
            ft,
            collections: IdMap::new(),
        }
    }

    pub fn get_text_size(
        &mut self,
        text: &str,
        size: isize,
        graphics: Arc<WlxGraphics>,
    ) -> (f32, f32) {
        let sizef = size as f32;

        let height = sizef + ((text.lines().count() as f32) - 1f32) * (sizef * 1.5);

        let mut max_w = sizef * 0.33;
        for line in text.lines() {
            let w: f32 = line
                .chars()
                .map(|c| {
                    self.get_glyph_for_cp(c as usize, size, graphics.clone())
                        .advance
                })
                .sum();

            if w > max_w {
                max_w = w;
            }
        }
        (max_w, height)
    }

    pub fn get_glyphs(
        &mut self,
        text: &str,
        size: isize,
        graphics: Arc<WlxGraphics>,
    ) -> Vec<Rc<Glyph>> {
        let mut glyphs = Vec::new();
        for line in text.lines() {
            for c in line.chars() {
                glyphs.push(self.get_glyph_for_cp(c as usize, size, graphics.clone()));
            }
        }
        glyphs
    }

    fn get_font_for_cp(&mut self, cp: usize, size: isize) -> usize {
        if !self.collections.contains_key(size) {
            self.collections.insert(
                size,
                FontCollection {
                    fonts: Vec::new(),
                    cp_map: IdMap::new(),
                },
            );
        }
        let coll = self.collections.get_mut(size).unwrap();

        if let Some(font) = coll.cp_map.get(cp) {
            return *font;
        }

        let pattern_str = format!("{PRIMARY_FONT}-{size}:style=bold:charset={cp:04x}");

        let mut pattern =
            OwnedPattern::from_str(&pattern_str).expect("Failed to create fontconfig pattern");
        self.fc
            .substitute(&mut pattern, fontconfig::MatchKind::Pattern);
        pattern.default_substitute();

        let pattern = pattern.font_match(&mut self.fc);

        if let Some(path) = pattern.filename() {
            log::debug!(
                "Loading font: {} {}pt",
                pattern.name().unwrap_or(path),
                size
            );

            let font_idx = pattern.face_index().unwrap_or(0);

            let face = self
                .ft
                .new_face(path, font_idx as _)
                .expect("Failed to load font face");
            face.set_char_size(size << 6, size << 6, 96, 96)
                .expect("Failed to set font size");

            let idx = coll.fonts.len();
            for cp in 0..0xFFFF {
                if coll.cp_map.contains_key(cp) {
                    continue;
                }
                let g = face.get_char_index(cp);
                if g > 0 {
                    coll.cp_map.insert(cp, idx);
                }
            }

            let zero_glyph = Rc::new(Glyph {
                tex: None,
                top: 0.,
                left: 0.,
                width: 0.,
                height: 0.,
                advance: size as f32 / 3.,
            });
            let mut glyphs = IdMap::new();
            glyphs.insert(0, zero_glyph);

            let font = Font { face, glyphs };
            coll.fonts.push(font);

            idx
        } else {
            coll.cp_map.insert(cp, 0);
            0
        }
    }

    fn get_glyph_for_cp(
        &mut self,
        cp: usize,
        size: isize,
        graphics: Arc<WlxGraphics>,
    ) -> Rc<Glyph> {
        let key = self.get_font_for_cp(cp, size);

        let font = &mut self.collections[size].fonts[key];

        if let Some(glyph) = font.glyphs.get(cp) {
            return glyph.clone();
        }

        if font.face.load_char(cp, LoadFlag::DEFAULT).is_err() {
            return font.glyphs[0].clone();
        }

        let glyph = font.face.glyph();
        if glyph.render_glyph(freetype::RenderMode::Normal).is_err() {
            return font.glyphs[0].clone();
        }

        let bmp = glyph.bitmap();
        let buf = bmp.buffer().to_vec();
        if buf.len() == 0 {
            return font.glyphs[0].clone();
        }

        let metrics = glyph.metrics();

        let format = match bmp.pixel_mode() {
            Ok(PixelMode::Gray) => Format::R8_UNORM,
            Ok(PixelMode::Gray2) => Format::R16_SFLOAT,
            Ok(PixelMode::Gray4) => Format::R32_SFLOAT,
            _ => return font.glyphs[0].clone(),
        };

        let mut cmd_buffer = graphics.create_command_buffer(CommandBufferUsage::OneTimeSubmit);
        let texture = cmd_buffer.texture2d(bmp.width() as _, bmp.rows() as _, format, &buf);
        cmd_buffer.build_and_execute_now();

        let g = Glyph {
            tex: Some(texture),
            top: (metrics.horiBearingY >> 6i64) as _,
            left: (metrics.horiBearingX >> 6i64) as _,
            advance: (metrics.horiAdvance >> 6i64) as _,
            width: bmp.width() as _,
            height: bmp.rows() as _,
        };

        font.glyphs.insert(cp, Rc::new(g));
        font.glyphs[cp].clone()
    }
}
