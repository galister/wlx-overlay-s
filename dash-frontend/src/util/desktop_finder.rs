use std::{collections::HashMap, ffi::OsStr, rc::Rc, sync::Arc, thread::JoinHandle, time::Instant};

use freedesktop::{xdg_data_dirs, xdg_data_home, ApplicationEntry};
use serde::{Deserialize, Serialize};

struct DesktopEntryOwned {
	exec_path: String,
	exec_args: String,
	app_name: String,
	icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
	pub exec_path: Rc<str>,
	pub exec_args: Rc<str>,
	pub app_name: Rc<str>,
	pub icon_path: Option<Rc<str>>,
}

impl From<DesktopEntryOwned> for DesktopEntry {
	fn from(value: DesktopEntryOwned) -> Self {
		Self {
			exec_path: value.exec_path.into(),
			exec_args: value.exec_args.into(),
			app_name: value.app_name.into(),
			icon_path: value.icon_path.map(|x| x.into()),
		}
	}
}

const CMD_BLACKLIST: [&str; 1] = [
	"lsp-plugins", // LSP Plugins collection. They clutter the application list a lot
];

const CATEGORY_TYPE_BLACKLIST: [&str; 5] = ["GTK", "Qt", "X-XFCE", "X-Bluetooth", "ConsoleOnly"];

pub struct DesktopFinder {
	size_preferences: Arc<[&'static OsStr]>,
	icon_folders: Arc<[String]>,
	entry_cache: Vec<DesktopEntry>,
	bg_task: Option<JoinHandle<Vec<DesktopEntryOwned>>>,
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

		let icon_folders: Arc<[String]> = icon_folders.into_iter().collect();

		let size_preferences: Arc<[&'static OsStr]> = ["scalable", "128x128", "96x96", "72x72", "64x64", "48x48", "32x32"]
			.into_iter()
			.map(OsStr::new)
			.collect();

		Self {
			size_preferences,
			icon_folders,
			entry_cache: Vec::new(),
			bg_task: None,
		}
	}

	fn build_cache(icon_folders: Arc<[String]>, size_preferences: Arc<[&'static OsStr]>) -> Vec<DesktopEntryOwned> {
		let start = Instant::now();

		let mut res = Vec::<DesktopEntryOwned>::new();
		'app_entries: for app_entry in ApplicationEntry::all() {
			let Some(app_entry_id) = app_entry.id() else {
				log::warn!(
					"No desktop entry id for application \"{}\"",
					app_entry.name().as_deref().unwrap_or("")
				);
				continue;
			};

			let Some(app_name) = app_entry.name() else {
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

			let icon_path = app_entry
				.icon()
				.and_then(|icon_name| Self::find_icon(&icon_folders, &size_preferences, &icon_name));

			for cat in app_entry.categories().unwrap_or_default() {
				if CATEGORY_TYPE_BLACKLIST.contains(&cat.as_str()) {
					continue 'app_entries;
				}
			}

			let entry = DesktopEntryOwned {
				app_name,
				exec_path,
				exec_args,
				icon_path,
			};

			res.push(entry);
		}
		log::debug!("App entry cache rebuild took {:?}", start.elapsed());

		res
	}

	fn find_icon(icon_folders: &[String], size_preferences: &[&'static OsStr], icon_name: &str) -> Option<String> {
		for folder in icon_folders {
			let pattern = format!("{}/hicolor/*/apps/{}.*", folder, icon_name);

			let mut entries: Vec<_> = glob::glob(&pattern)
				.expect("Bad glob pattern!")
				.filter_map(Result::ok)
				.collect();

			// sort by SIZE_PREFERENCES
			entries.sort_by_key(|path| {
				path
					.components()
					.rev()
					.nth(2) // ← hicolor/<*SIZE*>/apps/filename.ext
					.map(|c| c.as_os_str())
					.and_then(|size| size_preferences.iter().position(|&p| p == size))
					.unwrap_or(usize::MAX)
			});

			if let Some(first) = entries.into_iter().next() {
				return Some(first.to_string_lossy().into());
			}
		}
		None
	}

	fn wait_for_entries(&mut self) {
		let Some(bg_task) = self.bg_task.take() else {
			return;
		};

		let Ok(entries) = bg_task.join() else {
			return;
		};

		self.entry_cache.clear();
		for entry in entries {
			self.entry_cache.push(entry.into());
		}
	}

	pub fn find_entries(&mut self) -> Vec<DesktopEntry> {
		self.wait_for_entries();
		self.entry_cache.clone()
	}

	pub fn refresh(&mut self) {
		let bg_task = std::thread::spawn({
			let icon_folders = self.icon_folders.clone();
			let size_preferences = self.size_preferences.clone();
			move || Self::build_cache(icon_folders, size_preferences)
		});
		self.bg_task = Some(bg_task);
	}
}
