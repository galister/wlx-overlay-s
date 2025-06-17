#![allow(dead_code)]

use frame::{DrmFormat, WlxFrame};

pub mod frame;

#[cfg(feature = "wayland")]
pub mod wayland;

#[cfg(feature = "wlr")]
pub mod wlr_dmabuf;

#[cfg(feature = "wlr")]
pub mod wlr_screencopy;

#[cfg(feature = "pipewire")]
pub mod pipewire;

#[cfg(feature = "xshm")]
pub mod xshm;

pub trait WlxCapture<U, R> {
    fn init(
        &mut self,
        dmabuf_formats: &[DrmFormat],
        user_data: U,
        callback: fn(&U, WlxFrame) -> Option<R>,
    );
    fn is_ready(&self) -> bool;
    fn supports_dmbuf(&self) -> bool;
    fn receive(&mut self) -> Option<R>;
    fn pause(&mut self);
    fn resume(&mut self);
    fn request_new_frame(&mut self);
}
