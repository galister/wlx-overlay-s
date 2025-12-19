use std::rc::Rc;

use crate::assets::AssetProvider;

// a string which optionally has translation key in it
// it will hopefully support dynamic language changing soon
// for now it's just a simple string container
#[derive(Debug, Default, Clone)]
pub struct Translation {
	pub text: Rc<str>,
	pub translated: bool, // if true, `text` is a translation key
}

impl PartialEq for Translation {
	fn eq(&self, other: &Self) -> bool {
		*self.text == *other.text && self.translated == other.translated
	}
}

impl Translation {
	pub fn generate(&self, i18n: &mut I18n) -> Rc<str> {
		if self.translated {
			i18n.translate(&self.text)
		} else {
			self.text.clone()
		}
	}

	pub const fn from_raw_text_rc(text: Rc<str>) -> Self {
		Self {
			text,
			translated: false,
		}
	}

	pub fn from_raw_text_string(text: String) -> Self {
		Self {
			text: text.into(),
			translated: false,
		}
	}

	pub fn from_raw_text(text: &str) -> Self {
		Self {
			text: Rc::from(text),
			translated: false,
		}
	}

	pub fn from_translation_key(translated: &str) -> Self {
		Self {
			text: Rc::from(translated),
			translated: true,
		}
	}
}

pub struct I18n {
	json_root_translated: serde_json::Value, // any language
	json_root_fallback: serde_json::Value,   // english
}

fn find_translation<'a>(translation: &str, mut val: &'a serde_json::Value) -> Option<&'a str> {
	for part in translation.split('.') {
		if let Some(sub) = val.get(part) {
			val = sub;
		}
	}

	val.as_str()
}

fn guess_lang() -> String {
	if let Ok(lang) = std::env::var("LANG") {
		if let Some((first, _)) = lang.split_once('_') {
			String::from(first)
		} else {
			lang
		}
	} else {
		log::warn!("LANG is not set, defaulting to \"en\".");
		String::from("en")
	}
}

impl I18n {
	pub fn new(provider: &mut Box<dyn AssetProvider>) -> anyhow::Result<Self> {
		let mut lang = guess_lang();
		log::info!("Guessed system language: {lang}");

		match lang.as_str() {
			"en" | "pl" | "it" | "ja" | "es" | "de" => {}
			_ => {
				log::warn!("Unsupported language \"{}\", defaulting to \"en\".", lang.as_str());

				lang = String::from("en");
			}
		}

		let data_english = provider.load_from_path("lang/en.json")?;
		let data_translated = provider.load_from_path(&format!("lang/{lang}.json"))?;

		let json_root_fallback = serde_json::from_str(str::from_utf8(&data_english)?)?;
		let json_root_translated = serde_json::from_str(str::from_utf8(&data_translated)?)?;

		Ok(Self {
			json_root_translated,
			json_root_fallback,
		})
	}

	pub fn translate(&mut self, translation_key: &str) -> Rc<str> {
		if let Some(translated) = find_translation(translation_key, &self.json_root_translated) {
			return Rc::from(translated);
		}

		if let Some(translated_fallback) = find_translation(translation_key, &self.json_root_fallback) {
			log::warn!("missing translation for key \"{translation_key}\", using \"en\" instead");
			return Rc::from(translated_fallback);
		}

		log::error!("missing translation for key \"{translation_key}\"");
		Rc::from(translation_key) // show translation key as a fallback
	}

	pub fn translate_and_replace(&mut self, translation_key: &str, to_replace: (&str, &str)) -> String {
		let translated = self.translate(translation_key);
		translated.replace(to_replace.0, to_replace.1)
	}
}
