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

use std::{
    io::{stdout, IsTerminal},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use flexi_logger::FileSpec;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if stdout().is_terminal() {
        flexi_logger::Logger::try_with_env_or_str("info")?.start()?;
    } else {
        flexi_logger::Logger::try_with_env_or_str("info")?
            .log_to_file(FileSpec::default().directory("/tmp"))
            .start()?;
    }

    log::info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    #[cfg(feature = "openvr")]
    if std::env::args().any(|arg| arg == "--uninstall") {
        crate::backend::openvr::openvr_uninstall();
        return Ok(());
    }

    let running = Arc::new(AtomicBool::new(true));
    let _ = ctrlc::set_handler({
        let running = running.clone();
        move || {
            running.store(false, Ordering::Relaxed);
        }
    });

    auto_run(running);

    Ok(())
}

#[cfg(all(feature = "openxr", feature = "openvr"))]
fn auto_run(running: Arc<AtomicBool>) {
    use backend::common::BackendError;

    #[cfg(feature = "openxr")]
    {
        use crate::backend::openxr::openxr_run;
        let Err(BackendError::NotSupported) = openxr_run(running.clone()) else {
            return;
        };
    }

    #[cfg(feature = "openxr")]
    {
        use crate::backend::openvr::openvr_run;
        let Err(BackendError::NotSupported) = openvr_run(running) else {
            return;
        };
    }

    log::error!("No supported backends found");
}
