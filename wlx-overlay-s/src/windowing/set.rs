use slotmap::SecondaryMap;
use std::sync::Arc;
use wlx_common::windowing::OverlayWindowState;

use crate::windowing::OverlayID;

#[derive(Default)]
pub struct OverlayWindowSet {
    pub(super) name: Arc<str>,
    pub(super) overlays: SecondaryMap<OverlayID, OverlayWindowState>,
}
