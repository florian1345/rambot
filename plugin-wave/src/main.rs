use hound::{SampleFormat, WavReader};

use plugin_commons::file::{
    self,
    FileAudioSourceResolver,
    FilePluginConfigBuilder
};

use rambot_api::audio::AudioSource;
use rambot_api::audio::convert::{
    self,
    MonoAudioSource,
    StereoAudioSource,
    ResamplingAudioSource
};

use std::fs::File;

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

struct WaveAudioSourceResolver;

impl FileAudioSourceResolver<Box<dyn AudioSource + Send>>
for WaveAudioSourceResolver {
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
    let res = file::run_dyn_file_plugin(||
        FilePluginConfigBuilder::new()
            .with_audio_source_name("wave")
            .with_linked_file_extensions("wav")
            .build(), WaveAudioSourceResolver).await;

    match res {
        Ok(_) => {},
        Err(e) => eprintln!("{}", e)
    }
}
