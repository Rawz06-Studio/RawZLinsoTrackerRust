//! LinSoTracker's click sounds: `select` when an item turns on, `cancel` when
//! it turns off, at 30% volume.

const VOLUME: f32 = 0.3;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use rodio::Source;
    use std::io::Cursor;

    const SELECT: &[u8] = include_bytes!("../../../public/default/select.wav");
    const CANCEL: &[u8] = include_bytes!("../../../public/default/cancel.wav");

    #[derive(Default)]
    pub struct Sounds {
        /// Opened on the first sound, as some machines have no audio output.
        sink: Option<rodio::MixerDeviceSink>,
        failed: bool,
    }

    impl Sounds {
        pub fn play(&mut self, select: bool) {
            if self.sink.is_none() && !self.failed {
                match rodio::DeviceSinkBuilder::open_default_sink() {
                    Ok(mut sink) => {
                        sink.log_on_drop(false);
                        self.sink = Some(sink);
                    }
                    Err(error) => {
                        eprintln!("no audio output: {error}");
                        self.failed = true;
                    }
                }
            }
            let Some(sink) = &self.sink else {
                return;
            };
            let bytes = if select { SELECT } else { CANCEL };
            match rodio::Decoder::new(Cursor::new(bytes)) {
                Ok(sound) => sink.mixer().add(sound.amplify(super::VOLUME)),
                Err(error) => eprintln!("cannot play sound: {error}"),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    /// The browser plays each sound from its URL, so there is nothing to keep.
    #[derive(Default)]
    pub struct Sounds {
        _private: (),
    }

    impl Sounds {
        pub fn play(&mut self, select: bool) {
            let file = if select { "select" } else { "cancel" };
            if let Ok(audio) =
                web_sys::HtmlAudioElement::new_with_src(&format!("/default/{file}.wav"))
            {
                audio.set_volume(super::VOLUME as f64);
                // Browsers refuse sound until the page was clicked, which a
                // click on an item already is.
                let _ = audio.play();
            }
        }
    }
}

pub use imp::Sounds;
