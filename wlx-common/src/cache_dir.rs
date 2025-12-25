use std::{path::PathBuf, sync::LazyLock};

const FALLBACK_CACHE_PATH: &str = "/tmp/wayvr_cache";

static CACHE_ROOT_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
	if let Some(mut dir) = xdg::BaseDirectories::new().get_cache_home() {
		dir.push("wayvr");
		return dir;
	}

	//Return fallback cache path
	log::error!("Err: Failed to find cache path, using {FALLBACK_CACHE_PATH}");
	PathBuf::from(FALLBACK_CACHE_PATH)
});

fn get_cache_root() -> PathBuf {
	CACHE_ROOT_PATH.clone()
}

fn ensure_dir(cache_root_path: &PathBuf) {
	let _ = std::fs::create_dir(cache_root_path);
}

pub fn get_data(data_path: &str) -> Option<Vec<u8>> {
	let mut path = get_cache_root();
	ensure_dir(&path);
	path.push(data_path);
	std::fs::read(path).ok()
}

pub fn set_data(data_path: &str, data: &[u8]) -> std::io::Result<()> {
	let mut path = get_cache_root();
	path.push(data_path);
	std::fs::write(path, data)
}
