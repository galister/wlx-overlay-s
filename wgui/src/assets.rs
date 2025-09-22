use std::path::{Path, PathBuf};

pub trait AssetProvider {
	fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>>;
}

// replace "./foo/bar/../file.txt" with "./foo/file.txt"
pub fn normalize_path(path: &Path) -> PathBuf {
	let mut stack = Vec::new();
	for component in path.components() {
		match component {
			std::path::Component::ParentDir => {
				stack.pop();
			}
			std::path::Component::Normal(name) => {
				stack.push(name);
			}
			_ => {}
		}
	}
	stack.iter().collect()
}
