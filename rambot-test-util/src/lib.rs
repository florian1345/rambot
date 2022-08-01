#![cfg(feature = "testing")]

use rambot_api::{AudioSource, Sample};

use rand::{Rng, RngCore, SeedableRng};
use rand::distributions::Distribution;
use rand::rngs::SmallRng;

use rand_distr::{Normal, NormalError};

use std::io;

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
    rng: R
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
    /// `samples`. At any [AudioSource::read] request, a segment of the given
    /// `segment_size` is entered into the provided buffer, as long as both the
    /// number of remaining samples and the buffer size allow it.
    pub fn with_segment_size(samples: Vec<Sample>, segment_size: usize)
            -> MockAudioSource<ConstantDistribution, DummyRng> {
        MockAudioSource {
            samples,
            index: 0,
            segment_size_distribution: ConstantDistribution::new(segment_size),
            rng: DummyRng
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
            rng: SmallRng::from_rng(&mut rand::thread_rng()).unwrap()
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
            rng: SmallRng::from_rng(&mut rand::thread_rng()).unwrap()
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

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("mock audio source asked for child")
    }
}
