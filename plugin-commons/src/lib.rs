use rambot_api::{AudioSource, Sample};

use std::fs::File;
use std::io;
use std::io::BufReader;
use std::mem;
use std::path::Path;

struct ResamplingAudioSource<S> {
    base: S,
    buf: Vec<Sample>,
    step: f64,
    fraction: f64
}

impl<S> ResamplingAudioSource<S> {
    fn linear_combination(&self, frac_index: f64) -> Sample {
        let base = frac_index.floor() as usize;
        let fraction = frac_index.fract() as f32;

        self.buf[base] * (1.0 - fraction) +
            self.buf[base + 1] * fraction
    }
}

impl<S: AudioSource> ResamplingAudioSource<S> {
    fn read_maybe_zero(&mut self, buf: &mut [Sample])
            -> Result<Option<usize>, io::Error> {
        let required_base_buf_len =
            (buf.len() as f64 * self.step + self.fraction + 2.0)
                .floor() as usize;

        if required_base_buf_len > self.buf.len() {
            for _ in 0..(required_base_buf_len - self.buf.len()) {
                self.buf.push(Sample::ZERO);
            }
        }

        let base_sample_count =
            self.base.read(&mut self.buf[1..required_base_buf_len])?;

        if base_sample_count == 0 {
            return Ok(None);
        }

        let sample_count =
            ((base_sample_count as f64 - self.fraction) / self.step)
                .floor() as usize;
        let sample_count = sample_count.min(buf.len());

        for i in 0..sample_count {
            buf[i] =
                self.linear_combination(self.fraction + i as f64 * self.step);
        }

        self.buf[0] = self.buf[base_sample_count - 1];
        self.fraction =
            (self.fraction + sample_count as f64 * self.step).fract();

        Ok(Some(sample_count))
    }
}

impl<S: AudioSource> AudioSource for ResamplingAudioSource<S> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        loop {
            if let Some(count) = self.read_maybe_zero(buf)? {
                if count > 0 {
                    return Ok(count);
                }
            }
            else {
                return Ok(0);
            }
        }
    }

    fn has_child(&self) -> bool {
        self.base.has_child()
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        Box::new(ResamplingAudioSource {
            base: self.base.take_child(),
            buf: mem::take(&mut self.buf),
            step: self.step,
            fraction: self.fraction
        })
    }
}

const TARGET_SAMPLING_RATE: u32 = 48000;

/// Constructs an [AudioSource] trait object that emits the same audio as the
/// provided audio source, but transformed to the target sampling rate of
/// Discord of 48 kHz.
///
/// # Arguments
///
/// * `audio_source`: The [AudioSource] whose audio to transform.
/// * `sampling_rate`: The sampling rate of the given audio source.
///
/// # Returns
///
/// A boxed audio source which has a sampling rate of 48 kHz.
pub fn adapt_sampling_rate<S>(audio_source: S, sampling_rate: u32)
    -> Box<dyn AudioSource + Send>
where
    S: AudioSource + Send + 'static
{
    if sampling_rate == TARGET_SAMPLING_RATE {
        Box::new(audio_source)
    }
    else {
        Box::new(ResamplingAudioSource {
            base: audio_source,
            buf: Vec::new(),
            step: sampling_rate as f64 / TARGET_SAMPLING_RATE as f64,
            fraction: 0.0
        })
    }
}

/// Determines whether the given descriptor is the path of a file that has the
/// given extension. This is a common operation among plugins that read files,
/// as it is necessary for the implementation of various `can_resolve` methods.
///
/// # Arguments
///
/// * `descriptor`: The descriptor to check.
/// * `extension`: The required extension (including the period) in lower case.
///
/// # Returns
///
/// True if and only if the descriptor represents a file with the given
/// extension.
pub fn is_file_with_extension(descriptor: &str, extension: &str) -> bool {
    let file_extension = descriptor[(descriptor.len() - extension.len())..]
        .to_lowercase();

    file_extension == extension && Path::new(descriptor).exists()
}

/// Utility function for opening a file and wrapping it in a [BufReader]. Any
/// error is converted into a string to allow this function to be used inside
/// various `resolve` methods.
pub fn open_file_buf(file: &str) -> Result<BufReader<File>, String> {
    let file = File::open(file).map_err(|e| format!("{}", e))?;
    Ok(BufReader::new(file))
}
