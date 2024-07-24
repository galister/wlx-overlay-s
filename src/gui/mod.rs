use glam::Vec4;
use once_cell::sync::Lazy;

pub mod canvas;
pub mod font;
pub mod modular;

pub type GuiColor = Vec4;
pub(super) static FALLBACK_COLOR: Lazy<GuiColor> = Lazy::new(|| Vec4::new(1., 0., 1., 1.));

// Parses a color from a HTML hex string
pub fn color_parse(html_hex: &str) -> anyhow::Result<GuiColor> {
    if html_hex.len() == 7 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&html_hex[1..3], 16),
            u8::from_str_radix(&html_hex[3..5], 16),
            u8::from_str_radix(&html_hex[5..7], 16),
        ) {
            return Ok(Vec4::new(
                r as f32 / 255.,
                g as f32 / 255.,
                b as f32 / 255.,
                1.,
            ));
        }
    } else if html_hex.len() == 9 {
        if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
            u8::from_str_radix(&html_hex[1..3], 16),
            u8::from_str_radix(&html_hex[3..5], 16),
            u8::from_str_radix(&html_hex[5..7], 16),
            u8::from_str_radix(&html_hex[7..9], 16),
        ) {
            return Ok(Vec4::new(
                r as f32 / 255.,
                g as f32 / 255.,
                b as f32 / 255.,
                a as f32 / 255.,
            ));
        }
    }
    anyhow::bail!(
        "Invalid color string: '{}', must be 7 characters long (e.g. #FF00FF)",
        &html_hex
    )
}

pub enum KeyCapType {
    /// Label is in center of keycap
    Regular,
    /// Label on the top
    /// AltGr symbol on bottom
    RegularAltGr,
    /// Primary symbol on bottom
    /// Shift symbol on top
    Reversed,
    /// Primary symbol on bottom-left
    /// Shift symbol on top-left
    /// AltGr symbol on bottom-right
    ReversedAltGr,
}
