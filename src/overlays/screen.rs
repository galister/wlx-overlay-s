
pub struct ScreenInteractionData {
    next_scroll: Instant,
    next_move: Instant,
    mouse_transform: Affine2,
}
impl ScreenInteractionData {
    fn new(pos: Vec2, size: Vec2, transform: Transform) -> ScreenInteractionHandler {
        let transform = match transform {
            Transform::_90 | Transform::Flipped90 =>
                Affine2::from_cols(vec2(0., size.y), vec2(-size.x, 0.), vec2(pos.x + size.x, pos.y)),
            Transform::_180 | Transform::Flipped180 => 
                Affine2::from_cols(vec2(-size.x, 0.), vec2(0., -size.y), vec2(pos.x + size.x, pos.y + size.y)),
            Transform::_270 | Transform::Flipped270 => 
                Affine2::from_cols(vec2(0., -size.y), vec2(size.x, 0.), vec2(pos.x, pos.y + size.y)),
            _ => 
                Affine2::from_cols(vec2(size.x, 0.), vec2(0., size.y), pos),
        };

        ScreenInteractionHandler {
            next_scroll: Instant::now(),
            next_move: Instant::now(),
            mouse_transform: transform,
        }
    }
}

struct ScreenInteractionHandler {

}

