use slotmap::SecondaryMap;

use crate::windowing::{window::OverlayWindowState, OverlayID};

#[derive(Default)]
pub struct OverlayWindowSet {
    pub(super) overlays: SecondaryMap<OverlayID, OverlayWindowState>,
}
