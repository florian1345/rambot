use minimp3::{self, Decoder, Frame};

use rambot_api::{
    AdapterResolver,
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    Sample
};

use std::io::{self, ErrorKind, Read, Seek};

struct FrameIterator<R> {
    decoder: Decoder<R>
}

impl<R: Read> Iterator for FrameIterator<R> {
    type Item = Result<Frame, minimp3::Error>;

    fn next(&mut self) -> Option<Result<Frame, minimp3::Error>> {
        match self.decoder.next_frame() {
            Ok(frame) => Some(Ok(frame)),
            Err(minimp3::Error::Eof) => None,
            Err(e) => Some(Err(e))
        }
    }
}

fn to_f32(i: i16) -> f32 {
    const FACTOR: f32 = 1.0 / 32768.0;

    i as f32 * FACTOR
}

struct Mp3AudioSource<R: Read> {
    frames: FrameIterator<R>,
    current_frame: Frame,
    current_frame_idx: usize
}

impl<R: Read + Seek> AudioSource for Mp3AudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        if self.current_frame_idx >= self.current_frame.data.len() {
            if let Some(next_frame) = self.frames.next() {
                self.current_frame = next_frame
                    .map_err(|e|
                        io::Error::new(ErrorKind::Other, format!("{}", e)))?;
                self.current_frame_idx = 0;
            }
            else {
                return Ok(0);
            }
        }

        let channels = self.current_frame.channels;
        let remaining_data =
            &self.current_frame.data[self.current_frame_idx..];
        let sample_count = (remaining_data.len() / channels).min(buf.len());

        if channels == 1 {
            for sample_idx in 0..sample_count {
                let amp = to_f32(remaining_data[sample_idx]);

                buf[sample_idx] = Sample {
                    left: amp,
                    right: amp
                };
            }
        }
        else {
            for sample_idx in 0..sample_count {
                let data_idx = sample_idx * channels;
                let left = to_f32(remaining_data[data_idx]);
                let right = to_f32(remaining_data[data_idx + 1]);

                buf[sample_idx] = Sample {
                    left,
                    right
                }
            }
        }

        self.current_frame_idx += sample_count * channels;
        Ok(sample_count)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("mp3 audio source has no child")
    }
}

struct Mp3AudioSourceResolver;

impl AudioSourceResolver for Mp3AudioSourceResolver {

    fn can_resolve(&self, descriptor: &str) -> bool {
        plugin_commons::is_file_with_extension(descriptor, ".mp3")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let reader = plugin_commons::open_file_buf(descriptor)?;
        let decoder = Decoder::new(reader);
        let mut frames = FrameIterator {
            decoder
        };
        let first_frame = frames.next()
            .ok_or_else(|| "File is empty.".to_owned())?
            .map_err(|e| format!("{}", e))?;
        let sampling_rate = first_frame.sample_rate as u32;

        Ok(plugin_commons::adapt_sampling_rate(Mp3AudioSource {
            frames,
            current_frame: first_frame,
            current_frame_idx: 0
        }, sampling_rate))
    }
}

struct Mp3Plugin;

impl Plugin for Mp3Plugin {

    fn load_plugin(&self) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        vec![Box::new(Mp3AudioSourceResolver)]
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        Vec::new()
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        Vec::new()
    }

    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>> {
        Vec::new()
    }
}

fn make_mp3_plugin() -> Mp3Plugin {
    Mp3Plugin
}

rambot_api::export_plugin!(make_mp3_plugin);
