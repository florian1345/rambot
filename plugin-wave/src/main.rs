use hound::{SampleFormat, WavReader};

use plugin_commons::audio;
use plugin_commons::file::{
    self,
    FileAudioSourceResolver,
    FilePluginConfigBuilder
};

use rambot_api::audio::AudioSource;

use std::fs::File;

struct WaveAudioSourceResolver;

impl FileAudioSourceResolver<Box<dyn AudioSource + Send>>
for WaveAudioSourceResolver {
    fn resolve(&self, path: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = match File::open(path) {
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
                audio::source_from_format(
                    wav_reader.into_samples::<f32>()
                        .map(|r| r.unwrap()),
                    spec.channels,
                    spec.sample_rate),
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_value = 1u64 << (bits - 1);
                let factor = 1.0 / max_value as f32;
                audio::source_from_format(
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
