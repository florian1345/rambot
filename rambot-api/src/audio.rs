//! This module defines functionality and data structures related to audio
//! processing and streaming.

use serde::{Deserialize, Serialize};

use std::fmt::{self, Display, Formatter};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign, Mul, MulAssign, Div, DivAssign};

/// A single audio sample with left and right amplitude. Values are floats that
/// are normalized to between -1.0 and 1.0 (values outside that range are
/// permitted, but will lead to overmodulation if not sorted out before
/// playback).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Sample {

    /// The amplitude on the left speaker.
    pub left: f32,

    /// The amplitude on the right speaker.
    pub right: f32
}

impl Sample {

    /// A sample which is zero both on the left and right speakers.
    pub const ZERO: Sample = Sample {
        left: 0.0,
        right: 0.0
    };

    /// Creates a new sample from the given amplitudes.
    pub fn new(left: f32, right: f32) -> Sample {
        Sample {
            left,
            right
        }
    }
}

impl From<(f32, f32)> for Sample {
    fn from(pair: (f32, f32)) -> Sample {
        Sample::new(pair.0, pair.1)
    }
}

impl Display for Sample {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.left, self.right)
    }
}

impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Sample) {
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

impl SubAssign for Sample {
    fn sub_assign(&mut self, rhs: Sample) {
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

impl Sum<Sample> for Sample {
    fn sum<I: Iterator<Item = Sample>>(iter: I) -> Sample {
        let mut sum = Sample::ZERO;

        for sample in iter {
            sum += sample;
        }

        sum
    }
}

/// A trait for audio sources, i.e. structs which provide audio data in the
/// form of a stream of [Sample]s.
pub trait AudioSource {

    /// Gets the next [Sample] from this audio source. This is a blocking
    /// operation. The source shall return `None` if and only if it has reached
    /// the end.
    fn next(&mut self) -> Option<Sample>;
}

impl AudioSource for Box<dyn AudioSource> {
    fn next(&mut self) -> Option<Sample> {
        self.as_mut().next()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const EPS: f32 = 0.001;

    fn equal_within_eps(a: f32, b: f32) -> bool {
        a >= b - EPS && a <= b + EPS
    }

    #[test]
    fn sample_construction() {
        let s1 = Sample::new(0.5, 0.0);
        let s2 = Sample::new(0.0, 0.5);
        let s3 = Sample::from((0.5, 0.0));
        let s4 = Sample::from((0.0, 0.5));

        assert_eq!(s1, s3);
        assert_eq!(s2, s4);
        assert!(s1 != s4);
        assert!(s2 != s3);
    }

    #[test]
    fn sample_display() {
        let s = Sample::new(-0.25, 0.25);
        assert_eq!("(-0.25, 0.25)", &format!("{}", s));
    }

    #[test]
    fn sample_operations() {
        let s1 = Sample::new(0.5, 0.6);
        let s2 = Sample::new(-0.3, 0.1);
        let sum = s1 + s2;
        let diff = s1 - s2;
        let prod = s1 * 2.0;
        let quot = s2 / 2.0;

        assert!(equal_within_eps(sum.left, 0.2));
        assert!(equal_within_eps(sum.right, 0.7));
        assert!(equal_within_eps(diff.left, 0.8));
        assert!(equal_within_eps(diff.right, 0.5));
        assert!(equal_within_eps(prod.left, 1.0));
        assert!(equal_within_eps(prod.right, 1.2));
        assert!(equal_within_eps(quot.left, -0.15));
        assert!(equal_within_eps(quot.right, 0.05));
    }

    #[test]
    fn sample_sum() {
        let s1 = Sample::new(0.5, 0.6);
        let s2 = Sample::new(-0.3, 0.1);
        let s3 = Sample::new(0.1, 0.1);
        let sum: Sample = vec![s1, s2, s3].into_iter().sum();

        assert!(equal_within_eps(sum.left, 0.3));
        assert!(equal_within_eps(sum.right, 0.8));
    }
}
