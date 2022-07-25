use rambot_api::{AudioSource, Sample};

use std::io;
use std::mem;

struct ResamplingAudioSource<S> {
    base: S,
    buf: Vec<Sample>,
    step: usize,
    fraction_numerator: usize
}

impl<S> ResamplingAudioSource<S> {
    fn linear_combination(&self, frac_index: usize) -> Sample {
        let base = frac_index / TARGET_SAMPLING_RATE_USIZE;
        let rem = frac_index - base * TARGET_SAMPLING_RATE_USIZE;

        if rem > 0 {
            let fraction = rem as f32 / TARGET_SAMPLING_RATE_USIZE as f32;
    
            self.buf[base] * (1.0 - fraction) +
                self.buf[base + 1] * fraction
        }
        else {
            self.buf[base]
        }
    }
}

impl<S: AudioSource> ResamplingAudioSource<S> {
    fn read_maybe_zero(&mut self, buf: &mut [Sample])
            -> Result<Option<usize>, io::Error> {
        // TODO these computations are weird. maybe there is a way to make it
        // more obvious that they are correct?

        let required_base_buf_len =
            (self.fraction_numerator + (buf.len() - 1) * self.step +
                TARGET_SAMPLING_RATE_USIZE - 1) /
                TARGET_SAMPLING_RATE_USIZE + 1;

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
            ((base_sample_count + 1) * TARGET_SAMPLING_RATE_USIZE -
                self.fraction_numerator - 1) / self.step + 1;
        let sample_count = sample_count.min(buf.len());

        for i in 0..sample_count {
            buf[i] = self.linear_combination(
                self.fraction_numerator + i * self.step);
        }

        self.buf[0] = self.buf[base_sample_count];
        self.fraction_numerator =
            (self.fraction_numerator + sample_count * self.step) -
            (base_sample_count * TARGET_SAMPLING_RATE_USIZE);

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
            fraction_numerator: self.fraction_numerator
        })
    }
}

const TARGET_SAMPLING_RATE: u32 = 48000;
const TARGET_SAMPLING_RATE_USIZE: usize = TARGET_SAMPLING_RATE as usize;

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
            step: sampling_rate as usize,
            fraction_numerator: TARGET_SAMPLING_RATE_USIZE
        })
    }
}

#[cfg(test)]
mod tests {

    // TODO reduce code duplication with rambot::audio unit tests

    use super::*;

    struct MockAudioSource {
        samples: Vec<Sample>,
        index: usize
    }

    impl MockAudioSource {
        fn new(samples: Vec<Sample>)-> MockAudioSource {
            MockAudioSource {
                samples,
                index: 0
            }
        }
    }

    impl AudioSource for MockAudioSource {
        fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
            let remaining = &self.samples[self.index..];
            let len = buf.len().min(remaining.len());
            self.index += len;

            buf[..len].copy_from_slice(&remaining[..len]);

            Ok(len)
        }

        fn has_child(&self) -> bool {
            false
        }

        fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
            panic!("mock audio source asked for child")
        }
    }

    fn assert_within_eps(a: f32, b: f32) {
        const EPS: f32 = 0.001;

        if (a - b).abs() > EPS {
            panic!("floats not within epsilon: {} and {}", a, b);
        }
    }

    fn assert_approximately_equal(expected: &[Sample], actual: &[Sample]) {
        assert_eq!(expected.len(), actual.len());

        let zipped = expected.iter().cloned().zip(actual.iter().cloned());

        for (expected, actual) in zipped {
            assert_within_eps(expected.left, actual.left);
            assert_within_eps(expected.right, actual.right);
        }
    }

    fn test_data(len: usize, step: f32) -> Vec<Sample> {
        let mut result = Vec::with_capacity(len);

        for i in 0..len {
            let x = step * i as f32;
            let left = x - x * x;
            let right = 1.0 - x + x * x;

            result.push(Sample {
                left,
                right
            });
        }

        result
    }

    fn segmented_query(resampled: &mut Box<dyn AudioSource + Send>, buf: &mut [Sample], segment_size: usize) -> usize {
        let mut total = 0;

        for i in 0..((buf.len() + segment_size - 1) / segment_size) {
            let start = i * segment_size;
            let end = (start + segment_size).min(buf.len());
            let count = resampled.read(&mut buf[start..end]).unwrap();

            total += count;

            if count < end - start {
                return total;
            }
        }

        return total;
    }

    #[test]
    fn resample_with_identical_sampling_rate_is_noop() {
        let data = test_data(100, 0.01);
        let mut resampled = adapt_sampling_rate(
            MockAudioSource::new(data.clone()),
            TARGET_SAMPLING_RATE);
        let mut buf = vec![Sample::ZERO; 120];

        assert_eq!(100, resampled.read(&mut buf).unwrap());
        assert_approximately_equal(&data, &buf[..100]);
    }

    #[test]
    fn reduction_of_sampling_rate_works() {
        let to_resample = test_data(1200, 0.001);
        let mut resampled = adapt_sampling_rate(
            MockAudioSource::new(to_resample.clone()),
            TARGET_SAMPLING_RATE * 3 / 2);
        let mut buf = vec![Sample::ZERO; 1200];

        assert_eq!(800, segmented_query(&mut resampled, &mut buf, 77));
        assert_approximately_equal(&test_data(800, 0.0015), &buf[..800]);
    }

    #[test]
    fn increasing_sampling_rate_works() {
        let to_resample = test_data(1200, 0.00045);
        let mut resampled = adapt_sampling_rate(
            MockAudioSource::new(to_resample.clone()),
            TARGET_SAMPLING_RATE * 2 / 3);
        let mut buf = vec![Sample::ZERO; 2000];

        assert_eq!(1800, segmented_query(&mut resampled, &mut buf, 77));
        assert_approximately_equal(&test_data(1800, 0.0003),
            &buf[..1800]);
    }
}
