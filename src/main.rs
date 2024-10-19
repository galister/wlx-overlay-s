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

#[cfg(feature = "wayvr")]
mod config_wayvr;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use clap::Parser;
use flexi_logger::{Duplicate, FileSpec, LogSpecification};
use sysinfo::Pid;

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

    /// Path to write logs to
    #[arg(short, long, value_name = "FILE_PATH")]
    log_to: Option<String>,

    #[cfg(feature = "uidev")]
    /// Show a desktop window of a UI panel for development
    #[arg(short, long, value_name = "UI_NAME")]
    uidev: Option<String>,
}

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

    logging_init(&mut args)?;

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

    #[cfg(feature = "uidev")]
    if let Some(panel_name) = args.uidev.as_ref() {
        crate::backend::uidev::uidev_run(panel_name.as_str())?;
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
        match openxr_run(running.clone(), args.show) {
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
        match openvr_run(running.clone(), args.show) {
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

fn logging_init(args: &mut Args) -> anyhow::Result<()> {
    let log_file = args
        .log_to
        .take()
        .or_else(|| std::env::var("WLX_LOGFILE").ok())
        .or_else(|| Some("/tmp/wlx.log".to_string()));

    if let Some(log_to) = log_file.filter(|s| !s.is_empty()) {
        if let Err(e) = file_logging_init(&log_to) {
            log::error!("Failed to initialize file logging: {}", e);
            flexi_logger::Logger::try_with_env_or_str("info")?.start()?;
        }
    } else {
        flexi_logger::Logger::try_with_env_or_str("info")?.start()?;
    }

    log_panics::init();
    Ok(())
}

fn file_logging_init(log_to: &str) -> anyhow::Result<()> {
    let file_spec = FileSpec::try_from(PathBuf::from(log_to))?;
    let log_spec = LogSpecification::env_or_parse("info")?;

    let duplicate = log_spec
        .module_filters()
        .iter()
        .find(|m| m.module_name.is_none())
        .map(|m| match m.level_filter {
            log::LevelFilter::Trace => Duplicate::Trace,
            log::LevelFilter::Debug => Duplicate::Debug,
            log::LevelFilter::Info => Duplicate::Info,
            log::LevelFilter::Warn => Duplicate::Warn,
            _ => Duplicate::Error,
        });

    flexi_logger::Logger::with(log_spec)
        .log_to_file(file_spec)
        .duplicate_to_stderr(duplicate.unwrap_or(Duplicate::Error))
        .start()?;
    println!("Logging to: {}", log_to);
    Ok(())
}

fn ensure_single_instance(replace: bool) -> bool {
    let mut path = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    path.push("wlx-overlay-s.pid");

    if path.exists() {
        // load contents
        if let Ok(pid_str) = std::fs::read_to_string(&path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let mut system = sysinfo::System::new();
                system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]));
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
