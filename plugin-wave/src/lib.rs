use hound::{SampleFormat, WavIntoSamples, WavReader};

use rambot_api::{
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    Sample
};

use std::fs::File;
use std::io::{BufReader, ErrorKind, Read, self};
use std::path::Path;

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
    for i in 0..buf.len() {
        if let Some(sample) = f.next_sample() {
            let sample = sample.map_err(|e|
                io::Error::new(ErrorKind::Other, format!("{}", e)))?;
            buf[i] = sample;
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
}

struct WaveAudioSourceResolver;

impl AudioSourceResolver for WaveAudioSourceResolver {
    fn can_resolve(&self, descriptor: &str) -> bool {
        let extension = descriptor[(descriptor.len() - 4)..].to_lowercase();
        extension == ".wav" && Path::new(descriptor).exists()
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = File::open(descriptor).map_err(|e| format!("{}", e))?;
        let reader = BufReader::new(file);
        let wav_reader = WavReader::new(reader).map_err(|e| format!("{}", e))?;
        let spec = wav_reader.spec();

        match spec.sample_format {
            SampleFormat::Float => {
                Ok(Box::new(FloatWaveAudioSource {
                    samples: wav_reader.into_samples(),
                    channels: spec.channels
                }))
            },
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_value = 1u64 << (bits - 1);
                let factor = 1.0 / max_value as f32;

                Ok(Box::new(IntWaveAudioSource {
                    samples: wav_reader.into_samples(),
                    factor,
                    channels: spec.channels
                }))
            }
        }
    }
}

struct WavePlugin;

impl Plugin for WavePlugin {
    fn load_plugin(&self) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        vec![Box::new(WaveAudioSourceResolver)]
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        Vec::new()
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        Vec::new()
    }
}

fn make_wave_plugin() -> WavePlugin {
    WavePlugin
}

rambot_api::export_plugin!(make_wave_plugin);
