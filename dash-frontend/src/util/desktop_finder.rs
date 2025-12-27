use freedesktop::{ApplicationEntry, IconTheme};
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

const ICON_SIZE: u32 = 128;

const CMD_BLACKLIST: [&str; 1] = [
	"lsp-plugins", // LSP Plugins collection. They clutter the application list a lot
];

const CATEGORY_TYPE_BLACKLIST: [&str; 5] = ["GTK", "Qt", "X-XFCE", "X-Bluetooth", "ConsoleOnly"];

pub fn find_entries() -> anyhow::Result<Vec<DesktopEntry>> {
	let mut res = Vec::<DesktopEntry>::new();
	let theme = IconTheme::current();

	'outer: for app_entry in ApplicationEntry::all() {
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

		let icon_path = app_entry
			.icon()
			.and_then(|icon_name| theme.get_with_size(&icon_name, ICON_SIZE))
			.and_then(|path_buf| path_buf.into_os_string().into_string().ok());

		let categories = app_entry.categories().map_or_else(Vec::default, |inner| {
			inner
				.into_iter()
				.filter(|s| !(s.is_empty() || CATEGORY_TYPE_BLACKLIST.contains(&s.as_str())))
				.collect()
		});

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
