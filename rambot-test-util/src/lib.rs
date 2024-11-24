#![cfg(feature = "testing")]

use rambot_api::{
    AudioMetadata,
    AudioSource,
    AudioSourceList,
    Sample,
    AudioMetadataBuilder
};

use rand::{Rng, RngCore, SeedableRng};
use rand::distributions::Distribution;
use rand::rngs::SmallRng;

use rand_distr::{Normal, NormalError};

use std::f64::consts;
use std::io;
use std::vec::IntoIter;

/// An [Rng] implementation that panics any time it is used. This is designed
/// to be used with a [ConstantDistribution], as that requires no actual RNG
/// functionality to work.
pub struct DummyRng;

impl RngCore for DummyRng {

    fn next_u32(&mut self) -> u32 {
        panic!("dummy RNG used as RNG")
    }

    fn next_u64(&mut self) -> u64 {
        panic!("dummy RNG used as RNG")
    }

    fn fill_bytes(&mut self, _dest: &mut [u8]) {
        panic!("dummy RNG used as RNG")
    }

    fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
        panic!("dummy RNG used as RNG")
    }
}

/// A dummy [Distribution] implementation that returns a constant `usize` every
/// time it is sampled.
pub struct ConstantDistribution {
    constant: usize
}

impl ConstantDistribution {

    /// Creates a new constant distribution that always returns the given
    /// `constant` when sampled.
    pub fn new(constant: usize) -> ConstantDistribution {
        ConstantDistribution { constant }
    }
}

impl Distribution<usize> for ConstantDistribution {
    fn sample<R: Rng + ?Sized>(&self, _rng: &mut R) -> usize {
        self.constant
    }
}

/// A wrapper around a [Normal] distribution that rounds the result and returns
/// it as a `usize`. If the result is negative, the wrapped normal distribution
/// is sampled again until it returns a positive value. Note that this may
/// result in an effectively infinite loop if the wrapped normal distribution
/// effectively always returns negative numbers.
pub struct RoundedNormalDistribution {
    normal: Normal<f64>
}

impl RoundedNormalDistribution {

    /// Creates a new rounded normal distribution that wraps the given `normal`
    /// distribution.
    pub fn new(normal: Normal<f64>) -> RoundedNormalDistribution {
        RoundedNormalDistribution { normal }
    }
}

impl Distribution<usize> for RoundedNormalDistribution {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> usize {
        loop {
            let f = self.normal.sample(rng);

            if f >= 0.0 {
                return f.round() as usize;
            }
        }
    }
}

/// A mock [AudioSource] implementation for testing that returns a predefined
/// list of samples in segments of sizes controlled by a random distribution.
pub struct MockAudioSource<D, R> {
    samples: Vec<Sample>,
    index: usize,
    segment_size_distribution: D,
    rng: R,
    metadata: AudioMetadata
}

impl MockAudioSource<ConstantDistribution, DummyRng> {

    /// Creates a new mock audio source that streams the given list of
    /// `samples`. At any [AudioSource::read] request, the buffer is fillled up
    /// as much as possible with the remaining samples.
    pub fn new(samples: Vec<Sample>)
            -> MockAudioSource<ConstantDistribution, DummyRng> {
        MockAudioSource::with_segment_size(samples, usize::MAX)
    }

    /// Creates a new mock audio source that streams the given list of
    /// `samples`. At any [AudioSource::read] request, the buffer is fillled up
    /// as much as possible with the remaining samples. In addition, if
    /// [AudioSource::metadata] is requested, a clone of the given `metadata`
    /// is returned.
    pub fn with_metadata(samples: Vec<Sample>, metadata: AudioMetadata)
            -> MockAudioSource<ConstantDistribution, DummyRng> {
        MockAudioSource {
            samples,
            index: 0,
            segment_size_distribution: ConstantDistribution::new(usize::MAX),
            rng: DummyRng,
            metadata
        }
    }

    /// Creates a new mock audio source that streams the given list of
    /// `samples`. At any [AudioSource::read] request, a segment of the given
    /// `segment_size` is entered into the provided buffer, as long as both the
    /// number of remaining samples and the buffer size allow it.
    pub fn with_segment_size(samples: Vec<Sample>, segment_size: usize)
            -> MockAudioSource<ConstantDistribution, DummyRng> {
        MockAudioSource {
            samples,
            index: 0,
            segment_size_distribution: ConstantDistribution::new(segment_size),
            rng: DummyRng,
            metadata: AudioMetadataBuilder::new().build()
        }
    }
}

impl<D> MockAudioSource<D, SmallRng> {

    /// Creates a new mock audio source that streams the given list of
    /// `samples`. At any [AudioSource::read] request, the given
    /// `segment_size_distribution` is sampled to obtain a segment size. This
    /// amount of samples are entered into the given buffer, as long as both
    /// the number of remaining samples and the buffer size allow it. At least
    /// one sample is always entered, if any are remaining.
    pub fn with_segment_size_distribution(samples: Vec<Sample>,
            segment_size_distribution: D) -> MockAudioSource<D, SmallRng> {
        MockAudioSource {
            samples,
            index: 0,
            segment_size_distribution,
            rng: SmallRng::from_rng(&mut rand::thread_rng()).unwrap(),
            metadata: AudioMetadataBuilder::new().build()
        }
    }
}

impl MockAudioSource<RoundedNormalDistribution, SmallRng> {

    /// Creates a new mock audio source that streams the given list of
    /// `samples`. At any [AudioSource::read] request, a normal distribution
    /// with the given `mean` and standard deviation (`std_dev`) is sampled and
    /// the result rounded to obtain a segment size. This amount of samples are
    /// entered into the given buffer, as long as both the number of remaining
    /// samples and the buffer size allow it. At least one sample is always
    /// entered, if any are remaining.
    pub fn with_normally_distributed_segment_size(samples: Vec<Sample>,
            mean: f64, std_dev: f64)
            -> Result<MockAudioSource<RoundedNormalDistribution, SmallRng>,
                NormalError> {
        Ok(MockAudioSource {
            samples,
            index: 0,
            segment_size_distribution: RoundedNormalDistribution {
                normal: Normal::new(mean, std_dev)?
            },
            rng: SmallRng::from_rng(&mut rand::thread_rng()).unwrap(),
            metadata: AudioMetadataBuilder::new().build()
        })
    }
}

impl<D: Distribution<usize>, R: Rng> AudioSource for MockAudioSource<D, R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let remaining = &self.samples[self.index..];
        let len = self.segment_size_distribution.sample(&mut self.rng);
        let len = len.max(1).min(buf.len()).min(remaining.len());
        self.index += len;

        buf[..len].copy_from_slice(&remaining[..len]);

        Ok(len)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("mock audio source asked for child")
    }

    fn metadata(&self) -> AudioMetadata {
        self.metadata.clone()
    }
}

/// An [AudioSourceList] that outputs a predefined list of entries, for testing
/// purposes.
pub struct MockAudioSourceList {
    entries: IntoIter<String>
}

impl MockAudioSourceList {

    /// Creates a new mock audio source list that outputs the entries in the
    /// given collection in the order they are provided.
    pub fn new<S, I>(entries: I) -> MockAudioSourceList
    where
        I: IntoIterator<Item = S>,
        S: Into<String>
    {
        let entries = entries.into_iter()
            .map(|s| s.into())
            .collect::<Vec<_>>()
            .into_iter();

        MockAudioSourceList {
            entries
        }
    }
}

impl AudioSourceList for MockAudioSourceList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        Ok(self.entries.next())
    }
}

/// Reads audio from the given audio source until there is no more. The
/// returned vector of [Sample]s represents the complete remaining audio output
/// by the given source.
///
/// In addition to that, the sizes of buffers passed to the audio source for
/// filling are limited to `max_segment_size`. However, this function makes no
/// guarantee that they are not smaller.
///
/// # Errors
///
/// If the given audio source raises an error during reading.
pub fn read_to_end_segmented<S>(source: &mut S, max_segment_size: usize)
    -> Result<Vec<Sample>, io::Error>
where
    S: AudioSource
{
    let mut buf = vec![Sample::ZERO; 128];
    let mut len = 0;

    loop {
        if len >= buf.len() {
            buf.append(&mut vec![Sample::ZERO; len]);
        }

        let segment_size = max_segment_size.min(buf.len() - len);
        let end = len + segment_size;
        let count = source.read(&mut buf[len..end])?;

        if count == 0 {
            buf.truncate(len);
            return Ok(buf);
        }

        len += count;
    }
}

/// Reads audio from the given audio source until there is no more. The
/// returned vector of [Sample]s represents the complete remaining audio output
/// by the given source.
///
/// # Errors
///
/// If the given audio source raises an error during reading.
pub fn read_to_end<S>(source: &mut S) -> Result<Vec<Sample>, io::Error>
where
    S: AudioSource
{
    read_to_end_segmented(source, usize::MAX)
}

/// Asserts that both the given buffers have equal size. Further, asserts that
/// for every pair of samples with same indices, both the left and right
/// channels are within a small epsilon. If any of these conditions are
/// violated, the test fails.
pub fn assert_approximately_equal<S1, S2>(expected: S1, actual: S2)
where
    S1: AsRef<[Sample]>,
    S2: AsRef<[Sample]>
{
    fn assert_within_eps(a: f32, b: f32, i: usize) {
        const EPS: f32 = 0.0001;

        if (a - b).abs() > EPS {
            panic!("floats not within epsilon: {} and {} (index {})", a, b, i);
        }
    }

    let expected = expected.as_ref();
    let actual = actual.as_ref();

    assert_eq!(expected.len(), actual.len());

    let zipped = expected.iter().cloned().zip(actual.iter().cloned());

    for (i, (expected, actual)) in zipped.enumerate() {
        assert_within_eps(expected.left, actual.left, i);
        assert_within_eps(expected.right, actual.right, i);
    }
}

/// Collectes all entries in the given audio source `list` into a vector. Any
/// errors raised in [AudioSourceList::next] will be forwarded. If the list
/// contains more than `max_len` entries, only the first `max_len` entries are
/// queried and collected.
pub fn collect_list(list: &mut Box<dyn AudioSourceList + Send + Sync>,
        max_len: usize) -> Result<Vec<String>, io::Error> {
    let mut collected = Vec::new();

    for _ in 0..max_len {
        if let Some(entry) = list.next()? {
            collected.push(entry);
        }
        else {
            break;
        }
    }

    Ok(collected)
}

/// Generates test audio data consisting of one sine wave on each channel. The
/// two sine waves can have different frequencies.
///
/// # Arguments
///
/// * `len`: The number of generated samples, i.e. the length of the returned
///   data.
/// * `left_frequency`: The frequency of the sine wave played on the left
///   channel in Hz (where a sample rate of 48 kHz is assumed for the audio).
/// * `right_frequency`: The frequency of the sine wave played on the right
///   channel in Hz (where a sample rate of 48 kHz is assumed for the audio).
pub fn test_data(len: usize, left_frequency: f64, right_frequency: f64)
        -> Vec<Sample> {
    let mut data = Vec::with_capacity(len);

    for i in 0..len {
        let x = i as f64 / rambot_api::SAMPLES_PER_SECOND as f64;
        let left = (x * left_frequency * consts::TAU).sin() as f32;
        let right = (x * right_frequency * consts::TAU).sin() as f32;

        data.push(Sample {
            left,
            right
        })
    }

    data
}

fn random_frequency(rng: &mut impl Rng) -> f64 {
    40.0 * (rng.gen_range(0.0f64..5.0f64)).exp2()
}

/// Generates [test_data] with independently randomized frequencies on both
/// channels.
///
/// # Arguments
///
/// * `len`: The number of generated samples, i.e. the length of the returned
///   data.
pub fn random_test_data(len: usize) -> Vec<Sample> {
    let mut rng = rand::thread_rng();

    test_data(len, random_frequency(&mut rng), random_frequency(&mut rng))
}

/// Returns the element-wise sum of the two given sets of audio data. If one is
/// longer than the other, the first part where both data sets are defined will
/// be the sum and the rest of the longer audio data will be appended in the
/// end.
pub fn sum_audio(audio_1: &[Sample], audio_2: &[Sample]) -> Vec<Sample> {
    let (audio_1, audio_2) = if audio_1.len() < audio_2.len() {
        (audio_1, audio_2)
    }
    else {
        (audio_2, audio_1)
    };
    let mut sum = Vec::with_capacity(audio_2.len());

    for i in 0..audio_1.len() {
        sum.push(audio_1[i] + audio_2[i]);
    }

    for &sample in audio_2.iter().skip(audio_1.len()) {
        sum.push(sample);
    }

    sum
}

#[cfg(test)]
mod tests {

    use super::*;

    const RANDOM_TEST_ITERATIONS: usize = 64;
    const TEST_DATA_LEN: usize = 48000;

    #[test]
    fn test_data_continuous() {
        // At at most 1280 Hz we have one oscillation every 37.5 samples, which
        // is ~5.97 * 2 * PI. Hence, we expect the slope to be at most
        // ~1 / 5.97 and add a bit of buffer to be safe.

        const MAX_DIFF: f32 = 1.0 / 5.5;

        for _ in 0..RANDOM_TEST_ITERATIONS {
            let test_data = random_test_data(TEST_DATA_LEN);

            for (i, &sample) in test_data.iter().skip(1).enumerate() {
                let previous = test_data[i];
                let diff = sample - previous;

                assert!(diff.left.abs() < MAX_DIFF);
                assert!(diff.right.abs() < MAX_DIFF);
            }
        }
    }

    #[test]
    fn test_data_has_amplitude_1() {
        for _ in 0..RANDOM_TEST_ITERATIONS {
            let mut min_left = 0.0f32;
            let mut min_right = 0.0f32;
            let mut max_left = 0.0f32;
            let mut max_right = 0.0f32;
            let test_data = random_test_data(TEST_DATA_LEN);

            for sample in test_data {
                min_left = min_left.min(sample.left);
                min_right = min_right.min(sample.right);
                max_left = max_left.max(sample.left);
                max_right = max_right.max(sample.right);
            }

            assert!(min_left < -0.99);
            assert!(min_right < -0.99);
            assert!(max_left > 0.99);
            assert!(max_right > 0.99);
        }
    }
}
