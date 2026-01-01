use std::{collections::HashMap, io::Cursor};

use rodio::Source;

pub struct AudioSystem {
	audio_stream: Option<rodio::OutputStream>,
	first_try: bool,
}

pub struct AudioSample {
	buffer: rodio::buffer::SamplesBuffer,
}

pub struct SamplePlayer {
	samples: HashMap<String, AudioSample>,
}

impl SamplePlayer {
	pub fn new() -> Self {
		Self {
			samples: HashMap::new(),
		}
	}

	pub fn register_sample(&mut self, sample_name: &str, sample: AudioSample) {
		self.samples.insert(String::from(sample_name), sample);
	}

	pub fn play_sample(&mut self, system: &mut AudioSystem, sample_name: &str) {
		let Some(sample) = self.samples.get(sample_name) else {
			log::error!("failed to play sample by name {}", sample_name);
			return;
		};

		system.play_sample(sample);
	}
}

impl Default for SamplePlayer {
	fn default() -> Self {
		Self::new()
	}
}

impl AudioSystem {
	pub const fn new() -> Self {
		Self {
			audio_stream: None,
			first_try: true,
		}
	}

	fn get_handle(&mut self) -> Option<&rodio::OutputStream> {
		if self.audio_stream.is_none() && self.first_try {
			self.first_try = false;
			if let Ok(stream) = rodio::OutputStreamBuilder::open_default_stream() {
				self.audio_stream = Some(stream);
			} else {
				log::error!("Failed to open audio stream. Audio will not work.");
				return None;
			}
		}
		self.audio_stream.as_ref()
	}

	pub fn play_sample(&mut self, sample: &AudioSample) -> Option<()> {
		let handle = self.get_handle()?;
		handle.mixer().add(sample.buffer.clone());
		Some(())
	}
}

impl Default for AudioSystem {
	fn default() -> Self {
		Self::new()
	}
}

impl AudioSample {
	pub fn from_mp3(encoded_bin: &[u8]) -> anyhow::Result<Self> {
		// SAFETY: this is safe
		// rodio requires us to provide 'static data to decode it
		// we are casting &T into &'static T just to prevent unnecessary memory copy into Vec<u8>.
		// `encoded_bin` data will be always valid, because we are dropping `decoder` in this scope afterwards.
		// Compliant and slower version would be: Cursor::new(encoded_bin.to_vec())
		let cursor = unsafe { Cursor::new(std::mem::transmute::<&[u8], &'static [u8]>(encoded_bin)) };

		let decoder = rodio::Decoder::new_mp3(cursor)?;
		Ok(Self {
			buffer: rodio::buffer::SamplesBuffer::new(
				decoder.channels(),
				decoder.sample_rate(),
				decoder.collect::<Vec<rodio::Sample>>(),
			),
		})
	}
}
