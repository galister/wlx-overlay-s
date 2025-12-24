#[cfg(feature = "wayvr")]
use crate::backend::wayvr;

#[derive(Clone)]
pub enum WayVRSignal {
    #[cfg(feature = "wayvr")]
    DisplayVisibility(wayvr::display::DisplayHandle, bool),
    #[cfg(feature = "wayvr")]
    DisplayWindowLayout(
        wayvr::display::DisplayHandle,
        wayvr_ipc::packet_server::WvrDisplayWindowLayout,
    ),
    #[cfg(feature = "wayvr")]
    BroadcastStateChanged(wayvr_ipc::packet_server::WvrStateChanged),
    #[cfg(feature = "wayvr")]
    Haptics(crate::backend::input::Haptics),
    DeviceHaptics(usize, crate::backend::input::Haptics),
    DropOverlay(crate::windowing::OverlayID),
    CustomTask(crate::backend::task::ModifyPanelTask),
}
