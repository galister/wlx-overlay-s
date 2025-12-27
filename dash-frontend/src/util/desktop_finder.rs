use std::{collections::HashMap, ffi::OsStr, rc::Rc};

use freedesktop::{xdg_data_dirs, xdg_data_home, ApplicationEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
	pub exec_path: Rc<str>,
	pub exec_args: Rc<str>,
	pub app_name: Rc<str>,
	pub icon_path: Option<Rc<str>>,
}

const CMD_BLACKLIST: [&str; 1] = [
	"lsp-plugins", // LSP Plugins collection. They clutter the application list a lot
];

const CATEGORY_TYPE_BLACKLIST: [&str; 5] = ["GTK", "Qt", "X-XFCE", "X-Bluetooth", "ConsoleOnly"];

pub struct DesktopFinder {
	size_preferences: Vec<&'static OsStr>,
	icon_cache: HashMap<String, Rc<str>>,
	icon_folders: Vec<String>,
}

impl DesktopFinder {
	pub fn new() -> Self {
		let data_home = xdg_data_home();

		let mut icon_folders = vec![
			// XDG_DATA_HOME takes priority
			{
				let mut data_home_flatpak = data_home.clone();
				data_home_flatpak.push("flatpak/exports/share/icons");
				data_home_flatpak.to_string_lossy().to_string()
			},
			{
				let mut data_home = data_home.clone();
				data_home.push("icons");
				data_home.to_string_lossy().to_string()
			},
			"/var/lib/flatpak/exports/share/icons".into(),
		];

		let data_dirs = xdg_data_dirs();
		for mut data_dir in data_dirs {
			data_dir.push("icons");
			icon_folders.push(data_dir.to_string_lossy().to_string());
		}

		let size_preferences = ["scalable", "128x128", "96x96", "72x72", "64x64", "48x48", "32x32"]
			.into_iter()
			.map(OsStr::new)
			.collect();

		Self {
			icon_folders,
			icon_cache: HashMap::new(),
			size_preferences,
		}
	}

	fn find_icon(&mut self, icon_name: &str) -> Option<Rc<str>> {
		if let Some(icon_path) = self.icon_cache.get(icon_name) {
			return Some(icon_path.clone());
		}

		for folder in &self.icon_folders {
			let pattern = format!("{}/*/apps/*/{}.*", folder, icon_name);

			let mut entries: Vec<_> = glob::glob(&pattern)
				.expect("Bad glob pattern!")
				.filter_map(Result::ok)
				.collect();

			log::warn!("Looking for '{pattern}' resulted in {} entries.", entries.len());

			// sort by SIZE_PREFERENCES
			entries.sort_by_key(|path| {
				path
					.components()
					.rev()
					.nth(1) // ← <THEME>/apps/<*SIZE*>/filename.ext
					.map(|c| c.as_os_str())
					.and_then(|size| {
						log::warn!("looking for {size:?} in size preferences.");
						self.size_preferences.iter().position(|&p| p == size)
					})
					.unwrap_or(usize::MAX)
			});

			if let Some(first) = entries.into_iter().next() {
				let rc: Rc<str> = first.to_string_lossy().into();
				log::warn!("Found icon for {icon_name} at {rc}");
				self.icon_cache.insert(icon_name.to_string(), rc.clone());
				return Some(rc);
			}
		}
		None
	}

	pub fn find_entries(&mut self) -> anyhow::Result<Vec<DesktopEntry>> {
		let mut res = Vec::<DesktopEntry>::new();

		'app_entries: for app_entry in ApplicationEntry::all() {
			let Some(app_entry_id) = app_entry.id() else {
				log::warn!(
					"No desktop entry id for application \"{}\"",
					app_entry.name().as_deref().unwrap_or("")
				);
				continue;
			};

			let Some(name) = app_entry.name() else {
				log::warn!("No Name on desktop entry {}", app_entry_id);
				continue;
			};

			let Some(exec) = app_entry.exec() else {
				log::warn!("No Exec on desktop entry {}", app_entry_id);
				continue;
			};

			if app_entry.no_display() || app_entry.is_hidden() || app_entry.terminal() {
				continue;
			}

			for blacklisted in CMD_BLACKLIST {
				if exec.contains(blacklisted) {
					continue 'app_entries;
				}
			}

			let (exec_path, exec_args) = match exec.split_once(" ") {
				Some((left, right)) => (left.into(), right.into()),
				None => (exec.into(), "".into()),
			};

			let icon_path = app_entry.icon().and_then(|icon_name| self.find_icon(&icon_name));

			for cat in app_entry.categories().unwrap_or_default() {
				if CATEGORY_TYPE_BLACKLIST.contains(&cat.as_str()) {
					continue 'app_entries;
				}
			}

			let entry = DesktopEntry {
				app_name: name.into(),
				exec_path,
				exec_args,
				icon_path,
			};

			res.push(entry);
		}

		Ok(res)
	}
}
