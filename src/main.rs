#[allow(dead_code)]
mod backend;
mod config;
mod config_io;
mod graphics;
mod gui;
mod hid;
mod overlays;
mod shaders;
mod state;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use env_logger::Env;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    log::info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let running = Arc::new(AtomicBool::new(true));
    let _ = ctrlc::set_handler({
        let running = running.clone();
        move || {
            running.store(false, Ordering::Relaxed);
        }
    });

    #[cfg(all(feature = "openxr", feature = "openvr"))]
    auto_run(running);

    // TODO: Handle error messages if using cherry-picked features
    #[cfg(all(feature = "openvr", not(feature = "openxr")))]
    let _ = crate::backend::openvr::openvr_run(running);

    #[cfg(all(feature = "openxr", not(feature = "openvr")))]
    let _ = crate::backend::openxr::openxr_run(running);

    #[cfg(not(any(feature = "openxr", feature = "openvr")))]
    compile_error!("You must enable at least one backend feature (openxr or openvr)");
}

#[cfg(all(feature = "openxr", feature = "openvr"))]
fn auto_run(running: Arc<AtomicBool>) {
    use crate::backend::openvr::openvr_run;
    use crate::backend::openxr::openxr_run;
    use backend::common::BackendError;

    let Err(BackendError::NotSupported) = openxr_run(running.clone()) else {
        return;
    };

    let Err(BackendError::NotSupported) = openvr_run(running) else {
        return;
    };

    log::error!("No supported backends found");
}
