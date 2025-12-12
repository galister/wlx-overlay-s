use slotmap::SecondaryMap;
use std::sync::Arc;
use wlx_common::{astr_containers::AStrMap, windowing::OverlayWindowState};

use crate::windowing::OverlayID;

#[derive(Default)]
pub struct OverlayWindowSet {
    pub(super) name: Arc<str>,
    pub(super) overlays: SecondaryMap<OverlayID, OverlayWindowState>,

    // stores overlays that have not been seen since startup.
    pub(super) inactive_overlays: AStrMap<OverlayWindowState>,
}
