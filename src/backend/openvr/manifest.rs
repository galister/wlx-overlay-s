use std::{fs::File, io::Read};

use anyhow::bail;
use json::{array, object};
use ovr_overlay::applications::ApplicationsManager;

use crate::config_io::CONFIG_ROOT_PATH;

const APP_KEY: &str = "galister.wlxoverlay-s";

pub(super) fn install_manifest(app_mgr: &mut ApplicationsManager) -> anyhow::Result<()> {
    let executable_pathbuf = std::env::current_exe()?;
    let executable_path = executable_pathbuf
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?;
    let manifest_path = CONFIG_ROOT_PATH.join("wlx-overlay-s.vrmanifest");

    if let Ok(true) = app_mgr.is_application_installed(APP_KEY) {
        if let Ok(mut file) = File::open(&manifest_path) {
            let mut buf = String::new();
            if let Ok(_) = file.read_to_string(&mut buf) {
                let manifest: json::JsonValue = json::parse(&buf)?;
                if manifest["applications"][0]["binary_path_linux"] == executable_path {
                    log::info!("Manifest already up to date");
                    return Ok(());
                }
            }
        }
    }

    let manifest = object! {
        source: "builtin",
        applications: array![
            object! {
                app_key: APP_KEY,
                launch_type: "binary",
                binary_path_linux: executable_path,
                is_dashboard_overlay: true,
                strings: object!{
                    "en_us": object!{
                        name: "WlxOverlay-S",
                        description: "A lightweight Wayland desktop overlay for OpenVR/OpenXR",
                    },
                },
            },
        ],
    };

    let Ok(mut file) = File::create(&manifest_path) else {
        bail!("Failed to create manifest file at {:?}", manifest_path);
    };

    let Ok(()) = manifest.write(&mut file) else {
        bail!("Failed to write manifest file at {:?}", manifest_path);
    };

    let Ok(()) = app_mgr.add_application_manifest(&manifest_path, false) else {
        bail!("Failed to add manifest to OpenVR");
    };

    let Ok(()) = app_mgr.set_application_auto_launch(APP_KEY, true) else {
        bail!("Failed to set auto launch");
    };

    Ok(())
}

pub(super) fn uninstall_manifest(app_mgr: &mut ApplicationsManager) -> anyhow::Result<()> {
    let manifest_path = CONFIG_ROOT_PATH.join("wlx-overlay-s.vrmanifest");

    if let Ok(true) = app_mgr.is_application_installed(APP_KEY) {
        let Ok(()) = app_mgr.remove_application_manifest(&manifest_path) else {
            bail!("Failed to remove manifest from OpenVR");
        };
        log::info!("Uninstalled manifest");
    }
    Ok(())
}
