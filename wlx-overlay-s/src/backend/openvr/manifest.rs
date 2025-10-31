use std::{fs::File, io::Read};

use anyhow::bail;
use json::{array, object};
use ovr_overlay::applications::ApplicationsManager;

use crate::config_io;

const APP_KEY: &str = "galister.wlxoverlay-s";

pub(super) fn install_manifest(app_mgr: &mut ApplicationsManager) -> anyhow::Result<()> {
    let manifest_path = config_io::get_config_root().join("wlx-overlay-s.vrmanifest");

    let appimage_path = std::env::var("APPIMAGE");
    let executable_pathbuf = std::env::current_exe()?;

    let executable_path = match appimage_path {
        Ok(ref path) => path,
        Err(_) => executable_pathbuf
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?,
    };

    if app_mgr.is_application_installed(APP_KEY) == Ok(true)
        && let Ok(mut file) = File::open(&manifest_path)
    {
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_ok() {
            let manifest: json::JsonValue = json::parse(&buf)?;
            if manifest["applications"][0]["binary_path_linux"] == executable_path {
                log::info!("Manifest already up to date");
                return Ok(());
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
        bail!(
            "Failed to create manifest file at {}",
            manifest_path.display()
        );
    };

    if let Err(e) = manifest.write(&mut file) {
        bail!(
            "Failed to write manifest file at {}: {e:?}",
            manifest_path.display()
        );
    }

    if let Err(e) = app_mgr.add_application_manifest(&manifest_path, false) {
        bail!("Failed to add manifest to OpenVR: {}", e.description());
    }

    if let Err(e) = app_mgr.set_application_auto_launch(APP_KEY, true) {
        bail!("Failed to set auto launch: {}", e.description());
    }

    Ok(())
}

pub(super) fn uninstall_manifest(app_mgr: &mut ApplicationsManager) -> anyhow::Result<()> {
    let manifest_path = config_io::get_config_root().join("wlx-overlay-s.vrmanifest");

    if app_mgr.is_application_installed(APP_KEY) == Ok(true) {
        if let Err(e) = app_mgr.remove_application_manifest(&manifest_path) {
            bail!("Failed to remove manifest from OpenVR: {}", e.description());
        }
        log::info!("Uninstalled manifest");
    }
    Ok(())
}
