#[derive(Clone)]
pub enum WayVRSignal {
    BroadcastStateChanged(wayvr_ipc::packet_server::WvrStateChanged),
    DeviceHaptics(usize, crate::backend::input::Haptics),
    DropOverlay(crate::windowing::OverlayID),
    ShowHide,
    CustomTask(crate::backend::task::ModifyPanelTask),
}
