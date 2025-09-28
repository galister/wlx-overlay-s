use std::io::Cursor;

use rodio::{Decoder, OutputStreamBuilder, stream::OutputStream};

pub struct AudioOutput {
    audio_stream: Option<OutputStream>,
    first_try: bool,
}

impl AudioOutput {
    pub const fn new() -> Self {
        Self {
            audio_stream: None,
            first_try: true,
        }
    }

    fn get_handle(&mut self) -> Option<&OutputStream> {
        if self.audio_stream.is_none() && self.first_try {
            self.first_try = false;
            if let Ok(stream) = OutputStreamBuilder::open_default_stream() {
                self.audio_stream = Some(stream);
            } else {
                log::error!("Failed to open audio stream. Audio will not work.");
                return None;
            }
        }
        self.audio_stream.as_ref()
    }

    pub fn play(&mut self, wav_bytes: &'static [u8]) {
        let Some(handle) = self.get_handle() else {
            return;
        };
        let cursor = Cursor::new(wav_bytes);
        let source = match Decoder::new_wav(cursor) {
            Ok(source) => source,
            Err(e) => {
                log::error!("Failed to play sound: {e:?}");
                return;
            }
        };
        let _ = handle.mixer().add(source);
    }
}
