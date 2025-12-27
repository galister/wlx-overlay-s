use gio::prelude::{AppInfoExt, IconExt};
use gtk::traits::IconThemeExt;
use serde::{Deserialize, Serialize};

// compatibility with wayvr-ipc
// TODO: remove this after we're done with the old wayvr-dashboard and use DesktopEntry instead
#[derive(Debug, Deserialize, Serialize)]
pub struct DesktopFile {
	pub name: String,
	pub icon: Option<String>,
	pub exec_path: String,
	pub exec_args: Vec<String>,
	pub categories: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DesktopEntry {
	pub exec_path: String,
	pub exec_args: Vec<String>,
	pub app_name: String,
	pub icon_path: Option<String>,
	pub categories: Vec<String>,
}

#[allow(dead_code)] // TODO: remove this
pub struct EntrySearchCell {
	pub exec_path: String,
	pub exec_args: Vec<String>,
	pub app_name: String,
	pub icon_name: Option<String>,
	pub categories: Vec<String>,
}

const CMD_BLACKLIST: [&str; 1] = [
	"lsp-plugins", // LSP Plugins collection. They clutter the application list a lot
];

const CATEGORY_TYPE_BLACKLIST: [&str; 5] = ["GTK", "Qt", "X-XFCE", "X-Bluetooth", "ConsoleOnly"];

pub fn find_entries() -> anyhow::Result<Vec<DesktopEntry>> {
	let Some(icon_theme) = gtk::IconTheme::default() else {
		anyhow::bail!("Failed to get current icon theme information");
	};

	let mut res = Vec::<DesktopEntry>::new();

	let info = gio::AppInfo::all();

	log::debug!("app entry count {}", info.len());

	'outer: for app_entry in info {
		let Some(app_entry_id) = app_entry.id() else {
			log::warn!(
				"failed to get desktop entry ID for application named \"{}\"",
				app_entry.name()
			);
			continue;
		};

		let Some(desktop_app) = gio::DesktopAppInfo::new(&app_entry_id) else {
			log::warn!(
				"failed to find desktop app file from application named \"{}\"",
				app_entry.name()
			);
			continue;
		};

		if desktop_app.is_nodisplay() || desktop_app.is_hidden() {
			continue;
		}

		let Some(cmd) = desktop_app.commandline() else {
			continue;
		};

		let name = String::from(desktop_app.name());

		let exec = String::from(cmd.to_string_lossy());

		for blacklisted in CMD_BLACKLIST {
			if exec.contains(blacklisted) {
				continue 'outer;
			}
		}

		let (exec_path, exec_args) = match exec.split_once(" ") {
			Some((left, right)) => (
				String::from(left),
				right
					.split(" ")
					.filter(|arg| !arg.starts_with('%')) // exclude arguments like "%f"
					.map(String::from)
					.collect(),
			),
			None => (exec, Vec::new()),
		};

		let icon_path = match desktop_app.icon() {
			Some(icon) => {
				if let Some(icon_str) = icon.to_string() {
					if let Some(s_icon) = icon_theme.lookup_icon(&icon_str, 128, gtk::IconLookupFlags::GENERIC_FALLBACK) {
						s_icon.filename().map(|p| String::from(p.to_string_lossy()))
					} else {
						None
					}
				} else {
					None
				}
			}
			None => None,
		};

		let categories: Vec<String> = match desktop_app.categories() {
			Some(categories) => categories
				.split(";")
				.filter(|s| !s.is_empty())
				.filter(|s| {
					for b in CATEGORY_TYPE_BLACKLIST {
						if *s == b {
							return false;
						}
					}
					true
				})
				.map(String::from)
				.collect(),
			None => Vec::new(),
		};

		let entry = DesktopEntry {
			app_name: name,
			categories,
			exec_path,
			exec_args,
			icon_path,
		};

		res.push(entry);
	}

	Ok(res)
}

impl DesktopEntry {
	pub fn to_desktop_file(&self) -> DesktopFile {
		DesktopFile {
			categories: self.categories.clone(),
			exec_args: self.exec_args.clone(),
			exec_path: self.exec_path.clone(),
			icon: self.icon_path.clone(),
			name: self.app_name.clone(),
		}
	}
}
