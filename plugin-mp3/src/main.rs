use minimp3::{Decoder, Frame};

use rambot_api::audio::{AudioSource, Sample};
use rambot_api::audio::convert::{self, ResamplingAudioSource};
use rambot_api::plugin::{AudioSourceProvider, PluginAppBuilder, PluginBuilder};

use std::collections::VecDeque;
use std::fs::File;
use std::path::Path;

struct EmptyAudioSource;

impl AudioSource for EmptyAudioSource {
    fn next(&mut self) -> Option<Sample> {
        None
    }
}

fn next_frame(decoder: &mut Decoder<File>)
        -> Result<Option<Frame>, minimp3::Error> {
    match decoder.next_frame() {
        Ok(f) => Ok(Some(f)),
        Err(minimp3::Error::Eof) => Ok(None),
        Err(e) => Err(e)
    }
}

fn to_float(i: i16) -> f32 {
    i as f32 / 32768.0
}

struct Mp3AudioSource {
    current_frame: VecDeque<i16>,
    channels: usize,
    decoder: Decoder<File>
}

impl Mp3AudioSource {
    fn next_from_queue(&mut self) -> Option<Sample> {
        if let Some(left) = self.current_frame.pop_front() {
            let left = to_float(left);

            if self.channels == 1 {
                Some(Sample::new(left, left))
            }
            else {
                let right = to_float(self.current_frame.pop_front().unwrap());

                for _ in 2..self.channels {
                    self.current_frame.pop_front();
                }

                Some(Sample::new(left, right))
            }
        }
        else {
            None
        }
    }
}

impl AudioSource for Mp3AudioSource {
    fn next(&mut self) -> Option<Sample> {
        loop {
            if let Some(sample) = self.next_from_queue() {
                return Some(sample);
            }

            match next_frame(&mut self.decoder).unwrap() {
                Some(frame) => self.current_frame = VecDeque::from(frame.data),
                None => return None
            }
        }
    }
}

fn to_audio_source(mut decoder: Decoder<File>)
        -> Result<Box<dyn AudioSource + Send>, minimp3::Error> {
    let first_frame = next_frame(&mut decoder)?;

    if let Some(first_frame) = first_frame {
        let sample_rate = first_frame.sample_rate;
        let channels = first_frame.channels;
        let source = Mp3AudioSource {
            current_frame: VecDeque::from(first_frame.data),
            channels,
            decoder
        };

        if sample_rate == convert::REQUIRED_SAMPLING_RATE as i32 {
            Ok(Box::new(source))
        }
        else {
            let src_rate = sample_rate as f32;
            Ok(Box::new(
                ResamplingAudioSource::new_to_required(source, src_rate)))
        }
    }
    else {
        Ok(Box::new(EmptyAudioSource))
    }
}

struct Mp3AudioSourceProvider;

impl AudioSourceProvider<Box<dyn AudioSource + Send>>
for Mp3AudioSourceProvider {
    fn can_resolve(&self, code: &str) -> bool {
        code.to_lowercase().ends_with(".mp3") && Path::new(code).is_file()
    }

    fn resolve(&self, code: &str) -> Result<Box<dyn AudioSource + Send>, String> {
        let file = match File::open(code) {
            Ok(f) => f,
            Err(e) => return Err(format!("{}", e))
        };
        let decoder = Decoder::new(file);
        to_audio_source(decoder).map_err(|e| format!("{}", e))
    }
}

#[tokio::main]
async fn main() {
    let res = PluginAppBuilder::new()
        .with_plugin(PluginBuilder::new()
            .with_dyn_audio_source("mp3", Mp3AudioSourceProvider)
            .build())
        .build().launch().await;

    match res {
        Ok(_) => {},
        Err(e) => eprintln!("{}", e)
    }
}
