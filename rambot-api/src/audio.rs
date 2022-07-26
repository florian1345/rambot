use std::io;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

/// A single stereo audio sample in 32-bit float PCM format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {

    /// The current amplitude on the left channel. Usually on a scale from -1
    /// to 1.
    pub left: f32,

    /// The current amplitude on the right channel. Usually on a scale from -1
    /// to 1.
    pub right: f32
}

impl Sample {

    /// A sample which is zero on both channels.
    pub const ZERO: Sample = Sample {
        left: 0.0,
        right: 0.0
    };
}

impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl AddAssign<&Sample> for Sample {
    fn add_assign(&mut self, rhs: &Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl Add for Sample {
    type Output = Sample;

    fn add(mut self, rhs: Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add<&Sample> for Sample {
    type Output = Sample;

    fn add(mut self, rhs: &Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add for &Sample {
    type Output = Sample;

    fn add(self, rhs: &Sample) -> Sample {
        *self + rhs
    }
}

impl SubAssign for Sample {
    fn sub_assign(&mut self, rhs: Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl SubAssign<&Sample> for Sample {
    fn sub_assign(&mut self, rhs: &Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl Sub for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub<&Sample> for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: &Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub for &Sample {
    type Output = Sample;

    fn sub(self, rhs: &Sample) -> Sample {
        *self - rhs
    }
}

impl MulAssign<f32> for Sample {
    fn mul_assign(&mut self, rhs: f32) {
        self.left *= rhs;
        self.right *= rhs;
    }
}

impl Mul<f32> for Sample {
    type Output = Sample;

    fn mul(mut self, rhs: f32) -> Sample {
        self *= rhs;
        self
    }
}

impl Mul<f32> for &Sample {
    type Output = Sample;

    fn mul(self, rhs: f32) -> Sample {
        *self * rhs
    }
}

impl DivAssign<f32> for Sample {
    fn div_assign(&mut self, rhs: f32) {
        self.left /= rhs;
        self.right /= rhs;
    }
}

impl Div<f32> for Sample {
    type Output = Sample;

    fn div(mut self, rhs: f32) -> Sample {
        self /= rhs;
        self
    }
}

impl Div<f32> for &Sample {
    type Output = Sample;

    fn div(self, rhs: f32) -> Sample {
        *self / rhs
    }
}

/// A trait for types which can read audio data in the form of [Sample]s. The
/// interface is similar to that of the IO [Read](std::io::Read) trait.
pub trait AudioSource {

    /// Reads samples from this source into the given buffer. If the audio
    /// source offers any more data, at least one new sample must be written.
    /// Otherwise, it is assumed that the audio has finished. The buffer does
    /// not need to be filled completely even if there is more audio to come.
    /// The return value indicates how much data was read.
    ///
    /// # Arguments
    ///
    /// * `buf`: A [Sample] buffer to fill with data, starting from index 0.
    /// May or may not be filled with junk.
    ///
    /// # Returns
    ///
    /// The number of samples which were entered into the buffer. That is, this
    /// audio source generated samples from index 0 to one less than this
    /// number (inclusively).
    ///
    /// # Errors
    ///
    /// Any IO-[Error](io::Error) that occurs during reading.
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error>;

    /// Indicates whether this audio source wraps around a child source. This
    /// must be `true` for any audio source constituting an effect, i.e. which
    /// was resolved by an [EffectResolver](crate::resolver::EffectResolver).
    /// For example, a low-pass filter wraps around the root audio source which
    /// is filtered.
    fn has_child(&self) -> bool;

    /// Removes the child from this audio source and returns it. If
    /// [AudioSource::has_child] returns `true`, this must return a valid audio
    /// source, otherwise it may panic.
    ///
    /// Unless this method is called outside the framework, it is guaranteed
    /// that the audio source is dropped immediately afterwards. It is
    /// therefore not necessary to keep it in a usable state.
    fn take_child(&mut self) -> Box<dyn AudioSource + Send>;
}

impl AudioSource for Box<dyn AudioSource + Send> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        self.as_mut().read(buf)
    }

    fn has_child(&self) -> bool {
        self.as_ref().has_child()
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        self.as_mut().take_child()
    }
}

/// A trait for types which can offer a list or enumeration of descriptors,
/// such as a playlist or loop functionality.
pub trait AudioSourceList {

    /// Gets the next descriptor in the list, or `None` if the list is
    /// finished. May return an IO-[Error](io::Error) if the operation fails.
    fn next(&mut self) -> Result<Option<String>, io::Error>;
}
