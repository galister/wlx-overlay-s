use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use slotmap::SecondaryMap;

use crate::windowing::{window::OverlayWindowState, OverlayID};

#[derive(Default)]
pub struct OverlayWindowSet {
    pub(super) name: Arc<str>,
    pub(super) overlays: SecondaryMap<OverlayID, OverlayWindowState>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SerializedWindowSet {
    pub name: Arc<str>,
    pub overlays: HashMap<Arc<str>, OverlayWindowState>,
}
