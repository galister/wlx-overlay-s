use std::{path::PathBuf, sync::LazyLock};

const FALLBACK_CACHE_PATH: &str = "/tmp/wayvr_cache";

static CACHE_ROOT_PATH: LazyLock<PathBuf> = LazyLock::new(|| {

if let Some(mut dir) = xdg::BaseDirectories::new().get_cache_home() {
		dir.push("wayvr");
		return dir;
	}
	//Return fallback cache path
	log::error!("Err: Failed to find cache path, using {FALLBACK_CACHE_PATH}");
	PathBuf::from(FALLBACK_CACHE_PATH)	// Panics if neither $XDG_CACHE_HOME nor $HOME is set
});

fn get_cache_root() -> PathBuf {
	CACHE_ROOT_PATH.clone()
}

// todo: mutex
pub async fn get_data(data_path: &str) -> Option<Vec<u8>> {
	let mut path = get_cache_root();
	path.push(data_path);
	smol::fs::read(path).await.ok()
}

// todo: mutex
pub async fn set_data(data_path: &str, data: &[u8]) -> std::io::Result<()> {
	let mut path = get_cache_root();
	path.push(data_path);
	log::debug!(
		"Writing cache data ({} bytes) to path {}",
		data.len(),
		path.to_string_lossy()
	);

	let mut dir_path = path.clone();
	dir_path.pop();
	smol::fs::create_dir_all(dir_path).await?; // make sure directory is available
	smol::fs::write(path, data).await
}
