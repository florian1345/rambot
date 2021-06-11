use lewton::VorbisError;
use lewton::inside_ogg::OggStreamReader;

use rambot_api::audio::{AudioSource, Sample};
use rambot_api::audio::convert::{self, ResamplingAudioSource};
use rambot_api::plugin::{AudioSourceProvider, PluginAppBuilder, PluginBuilder};

use std::collections::VecDeque;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io;
use std::path::Path;

enum OggError {
    IOError(io::Error),
    VorbisError(VorbisError)
}

impl From<io::Error> for OggError {
    fn from(e: io::Error) -> OggError {
        OggError::IOError(e)
    }
}

impl From<VorbisError> for OggError {
    fn from(e: VorbisError) -> OggError {
        OggError::VorbisError(e)
    }
}

impl Display for OggError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            OggError::IOError(e) => write!(f, "{}", e),
            OggError::VorbisError(e) => write!(f, "{}", e)
        }
    }
}

struct OggAudioSource {
    reader: OggStreamReader<File>,
    samples: VecDeque<Sample>
}

impl OggAudioSource {
    fn new(path: &str) -> Result<OggAudioSource, OggError> {
        let file = File::open(path)?;
        let reader = OggStreamReader::new(file)?;
        Ok(OggAudioSource {
            reader,
            samples: VecDeque::new()
        })
    }

    fn sampling_rate(&self) -> u32 {
        self.reader.ident_hdr.audio_sample_rate
    }

    fn append_samples(&mut self, samples: impl Iterator<Item = Sample>) {
        for sample in samples {
            self.samples.push_back(sample);
        }
    }
}

impl AudioSource for OggAudioSource {
    fn next(&mut self) -> Option<Sample> {
        loop {
            if let Some(sample) = self.samples.pop_front() {
                return Some(sample);
            }

            let packet = self.reader.read_dec_packet_generic::<Vec<Vec<f32>>>()
                .unwrap();

            match packet {
                Some(packet) => {
                    if packet.len() == 1 {
                        let samples = packet[0].iter()
                            .map(|&f| Sample::new(f, f));
                        self.append_samples(samples);
                    }
                    else {
                        let samples = packet[0].iter()
                            .zip(packet[1].iter())
                            .map(|(&left, &right)| Sample::new(left, right));
                        self.append_samples(samples);
                    }
                },
                None => return None
            }
        }
    }
}

struct OggAudioSourceProvider;

impl AudioSourceProvider<Box<dyn AudioSource + Send>>
for OggAudioSourceProvider {
    fn can_resolve(&self, code: &str) -> bool {
        code.to_lowercase().ends_with(".ogg") && Path::new(code).is_file()
    }

    fn resolve(&self, code: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let source = match OggAudioSource::new(code) {
            Ok(s) => s,
            Err(e) => return Err(format!("{}", e))
        };
        let sampling_rate = source.sampling_rate();

        if sampling_rate == convert::REQUIRED_SAMPLING_RATE as u32 {
            Ok(Box::new(source))
        }
        else {
            let resampling_source =
                ResamplingAudioSource::new_to_required(
                    source,
                    sampling_rate as f32);
            Ok(Box::new(resampling_source))
        }
    }
}

#[tokio::main]
async fn main() {
    let errors = PluginAppBuilder::new()
        .with_plugin(PluginBuilder::new()
            .with_dyn_audio_source("ogg", OggAudioSourceProvider)
            .build())
        .build().launch().await;

    for e in errors {
        eprintln!("Error in plugin: {}", e);
    }
}
