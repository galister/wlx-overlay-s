use cosmic_text::PlatformFallback;
use parking_lot::Mutex;

use crate::i18n::Locale;

#[derive(Default)]
pub struct WguiFontConfig<'a> {
	pub binaries: Vec<&'a [u8]>,
	pub family_name_sans_serif: &'a str,
	pub family_name_serif: &'a str,
	pub family_name_monospace: &'a str,
}

pub struct WguiFontSystem {
	pub system: Mutex<cosmic_text::FontSystem>,
}

impl WguiFontSystem {
	pub fn new(config: &WguiFontConfig, locale: &Locale) -> Self {
		let mut db = cosmic_text::fontdb::Database::new();

		let system = if config.binaries.is_empty() {
			cosmic_text::FontSystem::new()
		} else {
			// needed for fallback
			db.load_system_fonts();

			for binary in &config.binaries {
				// binary data is copied and preserved here
				db.load_font_data(binary.to_vec());
			}

			if !config.family_name_sans_serif.is_empty() {
				db.set_sans_serif_family(config.family_name_sans_serif);
			}

			if !config.family_name_serif.is_empty() {
				db.set_serif_family(config.family_name_serif);
			}

			// we don't require anything special, at least for now
			cosmic_text::FontSystem::new_with_locale_and_db_and_fallback(
				locale.get_matched().to_owned(),
				db,
				PlatformFallback,
			)
		};

		Self {
			system: Mutex::new(system),
		}
	}
}
