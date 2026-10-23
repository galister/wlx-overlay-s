use std::{
	collections::{HashMap, HashSet},
	ffi::OsStr,
	fmt::Debug,
	fs::exists,
	path::Path,
	rc::Rc,
	sync::Arc,
	thread::JoinHandle,
	time::Instant,
};

use crate::{cache_dir, locale::Language};
use anyhow::Context;
use freedesktop_desktop_entry::DesktopEntry as FreedesktopDesktopEntry;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use wgui::i18n::Locale;

struct DesktopEntryOwned {
	app_id: Box<str>,
	exec_argv: Box<[Box<str>]>,
	app_name: Box<str>,
	icon_path: Option<Box<str>>,
	categories: Box<[Box<str>]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
	pub exec_argv: Rc<[Rc<str>]>,
	pub app_id: Rc<str>,
	pub app_name: Rc<str>,
	pub icon_path: Option<Rc<str>>,
	pub categories: Rc<[Rc<str>]>,
}

impl From<DesktopEntryOwned> for DesktopEntry {
	fn from(value: DesktopEntryOwned) -> Self {
		Self {
			app_id: value.app_id.into(),
			exec_argv: value.exec_argv.into_iter().map(|x| x.into()).collect(),
			app_name: value.app_name.into(),
			icon_path: value.icon_path.map(|x| x.into()),
			categories: value.categories.into_iter().map(|x| x.into()).collect(),
		}
	}
}

const CMD_BLOCKLIST: [&str; 3] = [
	"lsp-plugins", // LSP Plugins collection. They clutter the application list a lot
	"vrmonitor",
	"vrurlhandler",
];

const CATEGORY_TYPE_BLOCKLIST: [&str; 1] = ["ConsoleOnly"];

struct DesktopFinderParams {
	size_preferences: Vec<&'static OsStr>,
	icon_folders: Vec<String>,
	app_folders: Vec<String>,
	langs: Vec<String>,
}

pub struct DesktopFinder {
	params: Arc<DesktopFinderParams>,
	entry_cache: HashMap<String, Rc<DesktopEntry>>,
	bg_task: Option<JoinHandle<HashMap<String, DesktopEntryOwned>>>,
}

impl DesktopFinder {
	#[allow(clippy::new_without_default)]
	pub fn new(locale: &Locale) -> Self {
		let xdg = xdg::BaseDirectories::new();

		let mut app_folders = vec![];
		let mut icon_folders = vec![];

		if let Some(data_home) = xdg.get_data_home() {
			app_folders.push(data_home.join("applications").to_string_lossy().to_string());
			app_folders.push(
				data_home
					.join("flatpak/exports/share/applications")
					.to_string_lossy()
					.to_string(),
			);
			icon_folders.push(data_home.join("icons").to_string_lossy().to_string());
			icon_folders.push(
				data_home
					.join("flatpak/exports/share/icons")
					.to_string_lossy()
					.to_string(),
			);
		}

		app_folders.push("/var/lib/flatpak/exports/share/applications".into());
		icon_folders.push("/var/lib/flatpak/exports/share/icons".into());

		// /usr/share and such
		for data_dir in xdg.get_data_dirs() {
			app_folders.push(data_dir.join("applications").to_string_lossy().to_string());
			icon_folders.push(data_dir.join("icons").to_string_lossy().to_string());
		}

		let size_preferences: Vec<&'static OsStr> = ["scalable", "128x128", "96x96", "72x72", "64x64", "48x48", "32x32"]
			.into_iter()
			.map(OsStr::new)
			.collect();

		let langs = {
			let main_lang = locale.without_region().to_string();
			if main_lang == Language::get_default().code() {
				vec![main_lang]
			} else {
				vec![main_lang, Language::get_default().code().to_string()]
			}
		};

		Self {
			params: Arc::new(DesktopFinderParams {
				app_folders,
				icon_folders,
				size_preferences,
				langs,
			}),
			entry_cache: HashMap::new(),
			bg_task: None,
		}
	}

	fn build_cache(params: Arc<DesktopFinderParams>) -> HashMap<String, DesktopEntryOwned> {
		let start = Instant::now();

		let mut known_files = HashSet::new();
		let mut entries = HashMap::<String, DesktopEntryOwned>::new();

		let icons_folder = cache_dir::get_path("icons");
		if !std::fs::exists(&icons_folder).unwrap_or(false) {
			let _ = std::fs::create_dir(&icons_folder);
		}

		for path in &params.app_folders {
			log::debug!("Searching desktop entries in path {}", path);

			'entries: for entry in WalkDir::new(path)
				.into_iter()
				.filter_map(|e| e.ok())
				.filter(|e| !e.file_type().is_dir())
			{
				let Some(extension) = Path::new(entry.file_name()).extension() else {
					continue;
				};

				if extension != "desktop" {
					continue; // ignore, go on
				}

				let file_name = entry.file_name().to_string_lossy();
				let file_path = entry.path().display().to_string();

				let desktop = match FreedesktopDesktopEntry::from_path(entry.path(), Some(&params.langs)) {
					Ok(desktop) => desktop,
					Err(e) => {
						log::debug!("Failed to read .desktop file {}: {:?}, skipping", file_path, e);
						continue;
					}
				};

				if desktop.groups.desktop_entry().is_none() {
					log::debug!("Failed to get [Desktop Entry] section for file {}, skipping", file_path);
					continue;
				}

				// prefer entries from earlier data directories
				let app_id = desktop.id();
				if known_files.contains(app_id) {
					continue;
				}

				if desktop.only_show_in().is_some() || desktop.terminal() || desktop.no_display() || desktop.hidden() {
					continue;
				}

				let Some(exec) = desktop.exec() else {
					log::debug!("Failed to get desktop entry Exec for file {}, skipping", file_path);
					continue;
				};

				if CMD_BLOCKLIST.iter().any(|blocked| exec.contains(blocked)) {
					continue;
				}

				let exec_argv = desktop
					.parse_exec_with_uris(&[], &params.langs)
					.into_iter()
					.flatten()
					.map(|x| x.into_boxed_str())
					.collect::<Box<[_]>>();

				if exec_argv.is_empty() {
					log::debug!("Invalid Exec in desktop file {}, skipping", file_path);
					continue;
				};

				let Some(app_name) = desktop.name(&params.langs) else {
					log::debug!(
						"Failed to get desktop entry application name for file {}, skipping",
						file_path
					);
					continue;
				};

				let icon_path = desktop
					.icon()
					.and_then(|icon_name| Self::find_icon(&params, icon_name))
					.or_else(|| Self::create_icon(&file_name).ok());

				let mut vec_categories = Vec::<Box<str>>::new();
				for cat in desktop.categories().unwrap_or_default() {
					if CATEGORY_TYPE_BLOCKLIST.contains(&cat) {
						continue 'entries;
					}
					if !cat.is_empty() {
						vec_categories.push(cat.into());
					}
				}

				known_files.insert(app_id.to_owned());

				entries.insert(
					app_id.to_owned(),
					DesktopEntryOwned {
						app_id: app_id.into(),
						app_name: app_name.into(),
						exec_argv,
						icon_path: icon_path.map(|i| i.into_boxed_str()),
						categories: vec_categories.into_boxed_slice(),
					},
				);
			}
		}

		log::debug!("Desktop entry & icon scan took {:?}", start.elapsed());

		entries
	}

	fn find_icon(params: &DesktopFinderParams, icon_name: &str) -> Option<String> {
		if icon_name.starts_with("/") && exists(icon_name).unwrap_or(false) {
			return Some(icon_name.to_string());
		}

		for folder in &params.icon_folders {
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
					.and_then(|size| params.size_preferences.iter().position(|&p| p == size))
					.unwrap_or(usize::MAX)
			});

			if let Some(first) = entries.into_iter().next() {
				return Some(first.to_string_lossy().into());
			}
		}
		None
	}

	pub fn create_icon(desktop_entry_name: &str) -> anyhow::Result<String> {
		let relative_path = format!("icons/{}.svg", desktop_entry_name);
		let file_path = cache_dir::get_path(&relative_path);

		let mut dir_path = file_path.clone();
		dir_path.pop();
		std::fs::create_dir_all(dir_path).context("Could not create ~/.cache/wayvr")?;

		let file_path = file_path.to_string_lossy().to_string();

		if std::fs::exists(&file_path).unwrap_or(false) {
			return Ok(file_path);
		}

		let svg = identicons_svg::generate(identicons_svg::IdenticonOptions {
			background: identicons_svg::Background {
				r: 64,
				color: "rgba(0.9,0.9,0.9,0.5)".into(),
			},
			..Default::default()
		});

		std::fs::write(&file_path, svg).context("Could not save file to ~/.cache/wayvr")?;
		Ok(file_path)
	}

	fn wait_for_entries(&mut self) {
		let Some(bg_task) = self.bg_task.take() else {
			return;
		};

		let Ok(entries) = bg_task.join() else {
			return;
		};

		self.entry_cache.clear();
		for (app_id, entry) in entries {
			self.entry_cache.insert(app_id, Rc::new(entry.into()));
		}
	}

	pub fn get_cached_entry(&self, app_id: &str) -> Option<&Rc<DesktopEntry>> {
		self.entry_cache.get(app_id)
	}

	pub fn find_entries(&mut self) -> HashMap<String, Rc<DesktopEntry>> {
		self.wait_for_entries();
		self.entry_cache.clone()
	}

	pub fn refresh(&mut self) {
		let bg_task = std::thread::spawn({
			let params = self.params.clone();
			move || Self::build_cache(params)
		});
		self.bg_task = Some(bg_task);
	}
}
