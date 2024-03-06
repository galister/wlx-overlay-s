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

use clap::Parser;
use flexi_logger::FileSpec;

/// The lightweight desktop overlay for OpenVR and OpenXR
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[cfg(feature = "openvr")]
    /// Force OpenVR backend
    #[arg(long)]
    openvr: bool,

    #[cfg(feature = "openxr")]
    /// Force OpenXR backend
    #[arg(long)]
    openxr: bool,

    /// Uninstall OpenVR manifest and exit
    #[arg(long)]
    uninstall: bool,

    /// Path to write logs to
    #[arg(short, long, value_name = "FOLDER")]
    log_to: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if let Some(ref log_to) = args.log_to {
        flexi_logger::Logger::try_with_env_or_str("info")?
            .log_to_file(FileSpec::default().directory(log_to))
            .log_to_stdout()
            .start()?;
        println!("Logging to: {}", &log_to);
    } else {
        flexi_logger::Logger::try_with_env_or_str("info")?.start()?;
    }

    log::info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    #[cfg(feature = "openvr")]
    if args.uninstall {
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

    auto_run(running, args);

    Ok(())
}

fn auto_run(running: Arc<AtomicBool>, args: Args) {
    use backend::common::BackendError;

    #[cfg(feature = "openxr")]
    if !args_get_openvr(&args) {
        use crate::backend::openxr::openxr_run;
        match openxr_run(running.clone()) {
            Ok(()) => return,
            Err(BackendError::NotSupported) => (),
            Err(e) => {
                log::error!("{}", e.to_string());
                return;
            }
        };
    }

    #[cfg(feature = "openvr")]
    if !args_get_openxr(&args) {
        use crate::backend::openvr::openvr_run;
        match openvr_run(running.clone()) {
            Ok(()) => return,
            Err(BackendError::NotSupported) => (),
            Err(e) => {
                log::error!("{}", e.to_string());
                return;
            }
        };
    }

    log::error!("No more backends to try");

    #[cfg(not(any(feature = "openvr", feature = "openxr")))]
    compile_error!("No VR support! Enable either openvr or openxr features!");

    #[cfg(not(any(feature = "wayland", feature = "x11")))]
    compile_error!("No desktop support! Enable either wayland or x11 features!");
}

#[allow(dead_code)]
fn args_get_openvr(_args: &Args) -> bool {
    #[cfg(feature = "openvr")]
    let ret = _args.openvr;

    #[cfg(not(feature = "openvr"))]
    let ret = false;

    ret
}

#[allow(dead_code)]
fn args_get_openxr(_args: &Args) -> bool {
    #[cfg(feature = "openxr")]
    let ret = _args.openxr;

    #[cfg(not(feature = "openxr"))]
    let ret = false;

    ret
}
