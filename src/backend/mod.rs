pub mod common;
pub mod input;
pub mod notifications;

#[allow(clippy::all)]
mod notifications_dbus;

#[cfg(feature = "openvr")]
pub mod openvr;

#[cfg(feature = "openxr")]
pub mod openxr;

#[cfg(feature = "uidev")]
pub mod uidev;

#[cfg(feature = "osc")]
pub mod osc;

#[cfg(feature = "wayvr")]
pub mod wayvr;

pub mod overlay;

pub mod task;
