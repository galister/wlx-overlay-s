#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    dead_code,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::match_wildcard_for_single_variants,
    clippy::doc_markdown,
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    clippy::needless_pass_by_ref_mut,
    clippy::multiple_crate_versions
)]
mod backend;
mod config;
mod config_io;
mod graphics;
mod gui;
mod hid;
mod overlays;
mod shaders;
mod state;

#[cfg(feature = "wayvr")]
mod config_wayvr;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use backend::notifications::DbusNotificationSender;
use clap::Parser;
use sysinfo::Pid;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// The lightweight desktop overlay for OpenVR and OpenXR
#[derive(Default, Parser, Debug)]
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

    /// Show the working set of overlay on startup
    #[arg(long)]
    show: bool,

    /// Uninstall OpenVR manifest and exit
    #[arg(long)]
    uninstall: bool,

    /// Replace running WlxOverlay-S instance
    #[arg(long)]
    replace: bool,

    /// Allow multiple running instances of WlxOverlay-S (things may break!)
    #[arg(long)]
    multi: bool,

    /// Disable desktop access altogether.
    #[arg(long)]
    headless: bool,

    /// Path to write logs to
    #[arg(short, long, value_name = "FILE_PATH")]
    log_to: Option<String>,

    #[cfg(feature = "uidev")]
    /// Show a desktop window of a UI panel for development
    #[arg(short, long, value_name = "UI_NAME")]
    uidev: Option<String>,
}

#[allow(clippy::unnecessary_wraps)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = if std::env::args().skip(1).any(|a| !a.is_empty()) {
        Args::parse()
    } else {
        Args::default()
    };

    if !args.multi && !ensure_single_instance(args.replace) {
        println!("Looks like WlxOverlay-S is already running.");
        println!("Use --replace and I will terminate it for you.");
        return Ok(());
    }

    logging_init(&mut args);

    log::info!(
        "Welcome to {} version {}!",
        env!("CARGO_PKG_NAME"),
        env!("WLX_BUILD"),
    );
    log::info!("It is {}.", chrono::Local::now().format("%c"));

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

#[allow(unused_mut, clippy::similar_names)]
fn auto_run(running: Arc<AtomicBool>, args: Args) {
    use backend::common::BackendError;

    let mut tried_xr = false;
    let mut tried_vr = false;

    #[cfg(feature = "openxr")]
    if !args_get_openvr(&args) {
        use crate::backend::openxr::openxr_run;
        tried_xr = true;
        match openxr_run(running.clone(), args.show, args.headless) {
            Ok(()) => return,
            Err(BackendError::NotSupported) => (),
            Err(e) => {
                log::error!("{e:?}");
                return;
            }
        }
    }

    #[cfg(feature = "openvr")]
    if !args_get_openxr(&args) {
        use crate::backend::openvr::openvr_run;
        tried_vr = true;
        match openvr_run(running, args.show, args.headless) {
            Ok(()) => return,
            Err(BackendError::NotSupported) => (),
            Err(e) => {
                log::error!("{e:?}");
                return;
            }
        }
    }

    log::error!("No more backends to try");

    let instructions = match (tried_xr, tried_vr) {
        (true, true) => "Make sure that Monado, WiVRn or SteamVR is running.",
        (false, true) => "Make sure that SteamVR is running.",
        (true, false) => "Make sure that Monado or WiVRn is running.",
        _ => "Check your launch arguments.",
    };

    let instructions = format!("Could not connect to runtime.\n{instructions}");

    let _ = DbusNotificationSender::new()
        .and_then(|s| s.notify_send("WlxOverlay-S", &instructions, 1, 0, 0, false));

    #[cfg(not(any(feature = "openvr", feature = "openxr")))]
    compile_error!("No VR support! Enable either openvr or openxr features!");

    #[cfg(not(any(feature = "wayland", feature = "x11")))]
    compile_error!("No desktop support! Enable either wayland or x11 features!");
}

#[allow(dead_code, unused_variables)]
const fn args_get_openvr(args: &Args) -> bool {
    #[cfg(feature = "openvr")]
    let ret = args.openvr;

    #[cfg(not(feature = "openvr"))]
    let ret = false;

    ret
}

#[allow(dead_code, unused_variables)]
const fn args_get_openxr(args: &Args) -> bool {
    #[cfg(feature = "openxr")]
    let ret = args.openxr;

    #[cfg(not(feature = "openxr"))]
    let ret = false;

    ret
}

fn logging_init(args: &mut Args) {
    let log_file_path = args
        .log_to
        .take()
        .or_else(|| std::env::var("WLX_LOGFILE").ok())
        .unwrap_or_else(|| String::from("/tmp/wlx.log"));

    let file_writer = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_file_path)
    {
        Ok(file) => {
            println!("Logging to {}", &log_file_path);
            Some(file)
        }
        Err(e) => {
            println!("Failed to open log file (path: {e:?}): {log_file_path}");
            None
        }
    };

    let registry = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_writer(std::io::stderr),
        )
        .with(
            /* read RUST_LOG env var */
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("zbus=warn".parse().unwrap())
                .add_directive("wlx_capture::wayland=info".parse().unwrap())
                .add_directive("smithay=debug".parse().unwrap()), /* GLES render spam */
        );

    if let Some(writer) = file_writer {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(writer)
                    .with_ansi(false),
            )
            .init();
    } else {
        registry.init();
    }

    log_panics::init();
}

fn ensure_single_instance(replace: bool) -> bool {
    let mut path =
        std::env::var("XDG_RUNTIME_DIR").map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from);
    path.push("wlx-overlay-s.pid");

    if path.exists() {
        // load contents
        if let Ok(pid_str) = std::fs::read_to_string(&path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let mut system = sysinfo::System::new();
                system.refresh_processes(
                    sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
                    false,
                );
                if let Some(proc) = system.process(sysinfo::Pid::from_u32(pid)) {
                    if replace {
                        proc.kill_with(sysinfo::Signal::Term);
                        proc.wait();
                    } else {
                        return false;
                    }
                }
            }
        }
    }

    let pid = std::process::id().to_string();
    std::fs::write(path, pid).unwrap();

    true
}
