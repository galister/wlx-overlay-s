pub mod input;

#[cfg(feature = "openvr")]
pub mod openvr;

#[cfg(feature = "openxr")]
pub mod openxr;

#[cfg(feature = "wayvr")]
pub mod wayvr;

pub mod set;

pub mod task;

use thiserror::Error;

#[derive(Clone, Copy)]
pub enum XrBackend {
    OpenXR,
    OpenVR,
}

#[derive(Error, Debug)]
pub enum BackendError {
    #[error("backend not supported")]
    NotSupported,
    #[cfg(feature = "openxr")]
    #[error("OpenXR Error: {0:?}")]
    OpenXrError(#[from] ::openxr::sys::Result),
    #[error("Shutdown")]
    Shutdown,
    #[error("Restart")]
    Restart,
    #[error("Fatal: {0:?}")]
    Fatal(#[from] anyhow::Error),
}
