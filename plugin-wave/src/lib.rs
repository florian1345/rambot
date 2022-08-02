//! A plugin which can play back WAV files.

use hound::{SampleFormat, WavIntoSamples, WavReader};

use plugin_commons::{FileManager, OpenedFile};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSource,
    AudioSourceResolver,
    Plugin,
    PluginConfig,
    ResolverRegistry,
    Sample
};

use std::io::{ErrorKind, Read, self};

trait FloatSamples {
    fn next(&mut self);

    fn next_float(&mut self) -> Option<Result<f32, hound::Error>>;

    fn channels(&self) -> u16;

    #[inline]
    fn next_sample(&mut self) -> Option<Result<Sample, hound::Error>> {
        // TODO check whether rustc is smart enough to automatically take
        // this out of the loop.

        match self.next_float()? {
            Ok(sample) => {
                if self.channels() == 1 {
                    Some(Ok(Sample {
                        left: sample,
                        right: sample
                    }))
                }
                else {
                    let left = sample;
                    let right = match self.next_float().unwrap() {
                        Ok(s) => s,
                        Err(e) => return Some(Err(e))
                    };

                    for _ in 2..self.channels() {
                        self.next();
                    }

                    Some(Ok(Sample {
                        left,
                        right
                    }))
                }
            },
            Err(e) => Some(Err(e))
        }
    }
}

fn read<F>(f: &mut F, buf: &mut [Sample]) -> Result<usize, io::Error>
where
    F: FloatSamples
{
    for (i, buf_sample) in buf.iter_mut().enumerate() {
        if let Some(sample) = f.next_sample() {
            let sample = sample.map_err(|e|
                io::Error::new(ErrorKind::Other, format!("{}", e)))?;
            *buf_sample = sample;
        }
        else {
            return Ok(i);
        }
    }

    Ok(buf.len())
}

struct IntWaveAudioSource<R> {
    samples: WavIntoSamples<R, i32>,
    factor: f32,
    channels: u16
}

impl<R: Read> FloatSamples for IntWaveAudioSource<R> {

    #[inline]
    fn next(&mut self) {
        self.samples.next();
    }

    #[inline]
    fn next_float(&mut self) -> Option<Result<f32, hound::Error>> {
        let sample = match self.samples.next()? {
            Ok(s) => s,
            Err(e) => return Some(Err(e))
        };

        Some(Ok((sample as f32) * self.factor))
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels
    }
}

impl<R: Read> AudioSource for IntWaveAudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        read(self, buf)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("wave audio source has no child")
    }
}

struct FloatWaveAudioSource<R> {
    samples: WavIntoSamples<R, f32>,
    channels: u16
}

impl<R: Read> FloatSamples for FloatWaveAudioSource<R> {
    fn next(&mut self) {
        self.samples.next();
    }

    fn next_float(&mut self) -> Option<Result<f32, hound::Error>> {
        self.samples.next()
    }

    fn channels(&self) -> u16 {
        self.channels
    }
}

impl<R: Read> AudioSource for FloatWaveAudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        read(self, buf)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("wave audio source has no child")
    }
}

struct WaveAudioSourceResolver {
    file_manager: FileManager
}

impl WaveAudioSourceResolver {
    fn resolve_reader<R>(&self, reader: R)
        -> Result<Box<dyn AudioSource + Send>, String>
    where
        R: Read + Send + 'static
    {
        let wav_reader = WavReader::new(reader).map_err(|e| format!("{}", e))?;
        let spec = wav_reader.spec();

        match spec.sample_format {
            SampleFormat::Float => {
                Ok(plugin_commons::adapt_sampling_rate(FloatWaveAudioSource {
                    samples: wav_reader.into_samples(),
                    channels: spec.channels
                }, spec.sample_rate))
            },
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_value = 1u64 << (bits - 1);
                let factor = 1.0 / max_value as f32;

                Ok(plugin_commons::adapt_sampling_rate(IntWaveAudioSource {
                    samples: wav_reader.into_samples(),
                    factor,
                    channels: spec.channels
                }, spec.sample_rate))
            }
        }
    }
}

impl AudioSourceResolver for WaveAudioSourceResolver {

    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to a `.wav` file on the internet can be \
                provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Wave")
            .with_summary("Playback wave audio files.")
            .with_description(format!("Specify the path of a file with the \
                `.wav` extension relative to the bot root directory. {}This \
                plugin will playback the given file.", web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str) -> bool {
        self.file_manager.is_file_with_extension(descriptor, ".wav")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = self.file_manager.open_file_buf(descriptor)?;

        match file {
            OpenedFile::Local(reader) => self.resolve_reader(reader),
            OpenedFile::Web(reader) => self.resolve_reader(reader)
        }
    }
}

struct WavePlugin;

impl Plugin for WavePlugin {
    fn load_plugin<'registry>(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_audio_source_resolver(WaveAudioSourceResolver {
            file_manager: FileManager::new(config)
        });

        Ok(())
    }
}

fn make_wave_plugin() -> WavePlugin {
    WavePlugin
}

rambot_api::export_plugin!(make_wave_plugin);
