use claxon::{FlacReader, FlacSamples};
use claxon::input::BufferedReader;

use plugin_commons::audio;
use plugin_commons::file::{
    self,
    FileAudioSourceResolver,
    FilePluginConfigBuilder
};

use rambot_api::audio::AudioSource;

use std::fs::File;

#[ouroboros::self_referencing]
struct SamplesSelfRef {
    reader: Box<FlacReader<File>>,
    #[borrows(mut reader)]
    #[not_covariant]
    samples: FlacSamples<&'this mut BufferedReader<File>>
}

struct Samples(SamplesSelfRef);

impl Samples {
    fn new(reader: FlacReader<File>) -> Samples {
        Samples(SamplesSelfRefBuilder {
            reader: Box::new(reader),
            samples_builder: |reader| reader.samples()
        }.build())
    }
}

impl Iterator for Samples {
    type Item = claxon::Result<i32>;

    fn next(&mut self) -> Option<claxon::Result<i32>> {
        self.0.with_samples_mut(|samples| samples.next())
    }
}

struct FlacAudioSourceResolver;

impl FileAudioSourceResolver<Box<dyn AudioSource + Send>>
for FlacAudioSourceResolver {
    fn resolve(&self, path: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => return Err(format!("{}", e))
        };
        let reader = match FlacReader::new(file) {
            Ok(f) => f,
            Err(e) => return Err(format!("{}", e))
        };
        let info = reader.streaminfo();
        let factor = 1.0 / (1 << (info.bits_per_sample - 1)) as f32;
        let samples_f32 = Samples::new(reader)
            .map(move |i| i.unwrap() as f32 * factor);
        let channels = info.channels as u16;
        let sample_rate = info.sample_rate;

        audio::source_from_format(samples_f32, channels, sample_rate)
    }
}

#[tokio::main]
async fn main() {
    let res = file::run_dyn_file_plugin(||
        FilePluginConfigBuilder::new()
            .with_audio_source_name("flac")
            .with_linked_file_extensions("flac")
            .build(), FlacAudioSourceResolver).await;

    match res {
        Ok(_) => {},
        Err(e) => eprintln!("{}", e)
    }
}
