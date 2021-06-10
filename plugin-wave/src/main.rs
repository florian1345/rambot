use hound::{SampleFormat, WavReader};

use rambot_api::audio::AudioSource;
use rambot_api::audio::convert::{
    self,
    MonoAudioSource,
    StereoAudioSource,
    ResamplingAudioSource
};
use rambot_api::plugin::{AudioSourceProvider, PluginAppBuilder, PluginBuilder};

use std::fs::File;
use std::path::Path;

fn source_from_sample_rate<S>(s: S, sample_rate: u32)
    -> Box<dyn AudioSource + Send>
where
    S: AudioSource + Send + 'static
{
    let required = convert::REQUIRED_SAMPLING_RATE as u32;

    if sample_rate == required {
        Box::new(s)
    }
    else {
        Box::new(ResamplingAudioSource::new_to_required(s, sample_rate as f32))
    }
}

fn source_from_format<I>(iterator: I, channels: u16, sample_rate: u32)
    -> Result<Box<dyn AudioSource + Send>, String>
where
    I: Iterator<Item = f32> + Send + 'static
{
    match channels {
        1 => {
            let source = MonoAudioSource::new(iterator);
            Ok(source_from_sample_rate(source, sample_rate))
        },
        2 => {
            let source = StereoAudioSource::new(iterator);
            Ok(source_from_sample_rate(source, sample_rate))
        },
        _ => Err("Only mono and stereo files are supported.".to_owned())
    }
}

struct WaveAudioSourceProvider;

impl AudioSourceProvider<Box<dyn AudioSource + Send>>
for WaveAudioSourceProvider {

    fn can_resolve(&self, code: &str) -> bool {
        code.to_lowercase().ends_with(".wav") && Path::new(code).is_file()
    }

    fn resolve(&self, code: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = match File::open(code) {
            Ok(f) => f,
            Err(e) => return Err(format!("{}", e))
        };
        let wav_reader = match WavReader::new(file) {
            Ok(r) => r,
            Err(e) => return Err(format!("{}", e))
        };
        let spec = wav_reader.spec();
        match spec.sample_format {
            SampleFormat::Float =>
                source_from_format(
                    wav_reader.into_samples::<f32>()
                        .map(|r| r.unwrap()),
                    spec.channels,
                    spec.sample_rate),
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_value = 1u64 << (bits - 1);
                let factor = 1.0 / max_value as f32;
                source_from_format(
                    wav_reader.into_samples::<i32>()
                        .map(move |r| r.unwrap() as f32 * factor),
                    spec.channels,
                    spec.sample_rate)
            }
        }
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
