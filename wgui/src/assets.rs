use flate2::read::GzDecoder;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum AssetPath<'a> {
	WguiInternal(&'a str),  // tied to internal wgui AssetProvider. Used internally
	BuiltIn(&'a str),       // tied to user AssetProvider
	FileOrBuiltIn(&'a str), // attempts to load from a path relative to asset_folder, falls back to BuiltIn
	File(&'a str),          // load from filesystem
}

// see AssetPath above for documentation
#[derive(Debug, Clone)]
pub enum AssetPathOwned {
	WguiInternal(PathBuf),
	BuiltIn(PathBuf),
	FileOrBuiltIn(PathBuf),
	File(PathBuf),
}

impl AssetPath<'_> {
	pub const fn get_str(&self) -> &str {
		match &self {
			AssetPath::WguiInternal(path) => path,
			AssetPath::BuiltIn(path) => path,
			AssetPath::FileOrBuiltIn(path) => path,
			AssetPath::File(path) => path,
		}
	}

	pub fn to_owned(&self) -> AssetPathOwned {
		match self {
			AssetPath::WguiInternal(path) => AssetPathOwned::WguiInternal(PathBuf::from(path)),
			AssetPath::BuiltIn(path) => AssetPathOwned::BuiltIn(PathBuf::from(path)),
			AssetPath::FileOrBuiltIn(path) => AssetPathOwned::FileOrBuiltIn(PathBuf::from(path)),
			AssetPath::File(path) => AssetPathOwned::File(PathBuf::from(path)),
		}
	}
}

impl AssetPathOwned {
	pub fn as_ref(&'_ self) -> AssetPath<'_> {
		match self {
			AssetPathOwned::WguiInternal(buf) => AssetPath::WguiInternal(buf.to_str().unwrap()),
			AssetPathOwned::BuiltIn(buf) => AssetPath::BuiltIn(buf.to_str().unwrap()),
			AssetPathOwned::FileOrBuiltIn(buf) => AssetPath::FileOrBuiltIn(buf.to_str().unwrap()),
			AssetPathOwned::File(buf) => AssetPath::File(buf.to_str().unwrap()),
		}
	}

	pub const fn get_path_buf(&self) -> &PathBuf {
		match self {
			AssetPathOwned::WguiInternal(buf) => buf,
			AssetPathOwned::BuiltIn(buf) => buf,
			AssetPathOwned::FileOrBuiltIn(buf) => buf,
			AssetPathOwned::File(buf) => buf,
		}
	}
}

impl AssetPathOwned {
	#[must_use]
	pub fn push_include(&self, include: &str) -> AssetPathOwned {
		let buf = self.get_path_buf();
		let mut new_path = buf.parent().unwrap_or_else(|| Path::new("/")).to_path_buf();
		new_path.push(include);
		let new_path = normalize_path(&new_path);

		match self {
			AssetPathOwned::WguiInternal(_) => AssetPathOwned::WguiInternal(new_path),
			AssetPathOwned::BuiltIn(_) => AssetPathOwned::BuiltIn(new_path),
			AssetPathOwned::FileOrBuiltIn(_) => AssetPathOwned::FileOrBuiltIn(new_path),
			AssetPathOwned::File(_) => AssetPathOwned::File(new_path),
		}
	}
}

impl Default for AssetPathOwned {
	fn default() -> Self {
		Self::WguiInternal(PathBuf::default())
	}
}

pub trait AssetProvider {
	fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>>;
	fn load_from_path_gzip(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
		let compressed = self.load_from_path(path)?;
		let mut gz = GzDecoder::new(&compressed[..]);
		let mut out = Vec::new();
		gz.read_to_end(&mut out)?;
		Ok(out)
	}
}

// replace "./foo/bar/../file.txt" with "foo/file.txt"
pub fn normalize_path(path: &Path) -> PathBuf {
	let mut stack = Vec::new();

	for component in path.components() {
		match component {
			Component::ParentDir => {
				match stack.last() {
					// ../foo, ../../foo, ./../foo → push ".."
					None | Some(Component::ParentDir | Component::CurDir) => stack.push(Component::ParentDir),
					// "foo/../bar" → pop "foo" and don't push ".."
					Some(Component::Normal(_)) => {
						stack.pop();
					}
					// other weird cases, e.g. "/../foo" → "/foo"
					_ => {}
				}
			}
			// ./foo → foo
			Component::CurDir => {}

			// keep as-is
			Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
				stack.push(component);
			}
		}
	}

	stack
		.into_iter()
		.map(|comp| match comp {
			Component::RootDir => OsStr::new("/"),
			Component::Prefix(p) => p.as_os_str(), // should not occur on Unix
			Component::ParentDir => OsStr::new(".."),
			Component::Normal(s) => s,
			Component::CurDir => unreachable!(), // stripped in all cases
		})
		.collect()
}
