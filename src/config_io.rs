use log::error;
use once_cell::sync::Lazy;
use std::{
    fs::{self, create_dir},
    path::PathBuf,
};

const FALLBACK_CONFIG_PATH: &str = "/tmp/wlxoverlay";

pub static CONFIG_ROOT_PATH: Lazy<PathBuf> = Lazy::new(|| {
    if let Ok(xdg_dirs) = xdg::BaseDirectories::new() {
        let mut dir = xdg_dirs.get_config_home();
        dir.push("wlxoverlay");
        return dir;
    }
    //Return fallback config path
    error!(
        "Err: Failed to find config path, using {}",
        FALLBACK_CONFIG_PATH
    );
    PathBuf::from(FALLBACK_CONFIG_PATH)
});

pub fn get_conf_d_path() -> PathBuf {
    let mut config_root = CONFIG_ROOT_PATH.clone();
    config_root.push("conf.d");
    config_root
}

// Make sure config directory is present and return root config path
pub fn ensure_config_root() -> PathBuf {
    let path = CONFIG_ROOT_PATH.clone();
    let _ = create_dir(&path);

    let path_conf_d = get_conf_d_path();
    let _ = create_dir(path_conf_d);
    path
}

fn get_config_file_path(filename: &str) -> PathBuf {
    let mut config_root = CONFIG_ROOT_PATH.clone();
    config_root.push(filename);
    config_root
}

pub fn load(filename: &str) -> Option<String> {
    let path = get_config_file_path(filename);
    log::info!("Loading config {}", path.to_string_lossy());

    if let Ok(data) = fs::read_to_string(path) {
        Some(data)
    } else {
        None
    }
}
