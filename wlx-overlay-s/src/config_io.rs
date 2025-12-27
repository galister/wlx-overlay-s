use freedesktop::xdg_config_home;
use log::error;
use std::{path::PathBuf, sync::LazyLock};

pub enum ConfigRoot {
    Generic,
    #[allow(dead_code)]
    WayVR,
}

const FALLBACK_CONFIG_PATH: &str = "/tmp/wlxoverlay";

static CONFIG_ROOT_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    // Panics if $XDG_CONFIG_HOME and $HOME are both unset.
    let mut dir = xdg_config_home();
    dir.push("wlxoverlay");
    return dir;
});

pub fn get_config_root() -> PathBuf {
    CONFIG_ROOT_PATH.clone()
}

impl ConfigRoot {
    pub fn get_conf_d_path(&self) -> PathBuf {
        get_config_root().join(match self {
            Self::Generic => "conf.d",
            Self::WayVR => "wayvr.conf.d",
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

    std::fs::read_to_string(path).ok()
}
