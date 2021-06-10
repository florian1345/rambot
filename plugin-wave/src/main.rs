use hound::{SampleFormat, WavReader};

use rambot_api::audio::{AudioSource, Sample};
use rambot_api::plugin::{AudioSourceProvider, PluginAppBuilder, PluginBuilder};

use std::collections::VecDeque;
use std::fs::File;
use std::path::Path;

const BUFFER_SIZE: usize = 1024;

struct WaveAudioSource {
    reader: WavReader<File>,
    buffer: VecDeque<Sample>
}

impl WaveAudioSource {
    fn new(reader: WavReader<File>) -> WaveAudioSource {
        WaveAudioSource {
            reader,
            buffer: VecDeque::with_capacity(BUFFER_SIZE)
        }
    }
}

fn fill_buffer<I: Iterator<Item = f32>>(mut iterator: I, channels: u16,
        buffer: &mut VecDeque<Sample>) {
    while let Some(left) = iterator.next() {
        if channels > 1 {
            let right = iterator.next().unwrap();

            for _ in 2..channels {
                iterator.next();
            }

            buffer.push_back(Sample::new(left, right));
        }
        else {
            buffer.push_back(Sample::new(left, left));
        }

        if buffer.len() == BUFFER_SIZE {
            break;
        }
    }
}

impl AudioSource for WaveAudioSource {
    fn next(&mut self) -> Option<Sample> {
        if let Some(sample) = self.buffer.pop_front() {
            return Some(sample);
        }

        let spec = self.reader.spec();
        match spec.sample_format {
            SampleFormat::Float =>
                fill_buffer(self.reader.samples::<f32>()
                    .map(|s| s.unwrap()),
                    spec.channels, &mut self.buffer),
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_value = 1u64 << (bits - 1);
                let factor = 1.0 / max_value as f32;
                fill_buffer(self.reader.samples::<i32>()
                    .map(|s| s.unwrap() as f32 * factor),
                    spec.channels, &mut self.buffer)
            }
        }

        self.buffer.pop_front()
    }
}

struct WaveAudioSourceProvider;

impl AudioSourceProvider<WaveAudioSource> for WaveAudioSourceProvider {

    fn can_resolve(&self, code: &str) -> bool {
        code.to_lowercase().ends_with(".wav") && Path::new(code).is_file()
    }

    fn resolve(&self, code: &str) -> Result<WaveAudioSource, String> {
        let file = match File::open(code) {
            Ok(f) => f,
            Err(e) => return Err(format!("{}", e))
        };
        let wav_reader = match WavReader::new(file) {
            Ok(r) => r,
            Err(e) => return Err(format!("{}", e))
        };
        Ok(WaveAudioSource::new(wav_reader))
    }
}

#[tokio::main]
async fn main() {
    let errors = PluginAppBuilder::new()
        .with_plugin(PluginBuilder::new()
            .with_audio_source("wave", WaveAudioSourceProvider)
            .build())
        .build().launch().await;

    for e in errors {
        eprintln!("Error in plugin: {}", e);
    }
}
