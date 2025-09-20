use glam::Affine3A;
use std::sync::Arc;

pub struct OverlaySetItem {
    name: Arc<str>,
    transform: Affine3A,
}

pub struct OverlaySet {}
