#[allow(dead_code)]
mod backend;
mod graphics;
mod gui;
mod hid;
mod overlays;
mod shaders;
mod state;

use crate::backend::openvr::openvr_run;
use env_logger::Env;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    log::info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    openvr_run();
}
