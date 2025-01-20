use log::error;
use once_cell::sync::Lazy;
use std::path::PathBuf;

pub enum ConfigRoot {
    Generic,
    #[allow(dead_code)]
    WayVR,
}

const FALLBACK_CONFIG_PATH: &str = "/tmp/wlxoverlay";

static CONFIG_ROOT_PATH: Lazy<PathBuf> = Lazy::new(|| {
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

pub fn get_config_root() -> PathBuf {
    CONFIG_ROOT_PATH.clone()
}

impl ConfigRoot {
    pub fn get_conf_d_path(&self) -> PathBuf {
        get_config_root().join(match self {
            ConfigRoot::Generic => "conf.d",
            ConfigRoot::WayVR => "wayvr.conf.d",
        })
    }

    // Make sure config directory is present and return root config path
    pub fn ensure_dir(&self) -> PathBuf {
        let path = get_config_root();
        let _ = std::fs::create_dir(&path);

        let path_conf_d = self.get_conf_d_path();
        let _ = std::fs::create_dir(path_conf_d);
        path
    }
}

pub fn get_config_file_path(filename: &str) -> PathBuf {
    get_config_root().join(filename)
}

pub fn load(filename: &str) -> Option<String> {
    let path = get_config_file_path(filename);
    log::info!("Loading config: {}", path.to_string_lossy());

    if let Ok(data) = std::fs::read_to_string(path) {
        Some(data)
    } else {
        None
    }
}
