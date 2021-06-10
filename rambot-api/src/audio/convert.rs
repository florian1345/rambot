//! This module contains some utility for converting different formats of audio
//! streams, such as different channel counts and sampling rates.

use crate::audio::{AudioSource, Sample};

/// An [AudioSource] which wraps an iterator, which represents a single audio
/// channel, and emits mono audio accordingly.
pub struct MonoAudioSource<I: Iterator<Item = f32>> {
    iterator: I
}

impl<I: Iterator<Item = f32>> MonoAudioSource<I> {

    /// Creates a new mono audio source from the iterator representing the
    /// audio channel.
    pub fn new(iterator: I) -> MonoAudioSource<I> {
        MonoAudioSource { iterator }
    }
}

impl<I: Iterator<Item = f32>> AudioSource for MonoAudioSource<I> {
    fn next(&mut self) -> Option<Sample> {
        self.iterator.next().map(|f| Sample::new(f, f))
    }
}

/// An [AudioSource] which wraps an iterator, which interleaves two audio
/// channels in the order left-right-... and emits stereo audio accordingly.
pub struct StereoAudioSource<I: Iterator<Item = f32>> {
    iterator: I
}

impl<I: Iterator<Item = f32>> StereoAudioSource<I> {

    /// Creates a new stereo audio source from the interleaved channel
    /// iterator.
    pub fn new(iterator: I) -> StereoAudioSource<I> {
        StereoAudioSource { iterator }
    }
}

impl<I: Iterator<Item = f32>> AudioSource for StereoAudioSource<I> {
    fn next(&mut self) -> Option<Sample> {
        let left = self.iterator.next()?;
        let right = self.iterator.next()?;
        Some(Sample::new(left, right))
    }
}

/// This [AudioSource] transforms another audio source which emits audio at
/// some sampling rate into one with another sampling rate by interpolation
/// with linear splines.
pub struct ResamplingAudioSource<S: AudioSource> {
    source: S,
    step: f32,
    offset: f32,
    previous: Sample,
    next: Sample
}

/// The sampling rate that is required to match with the Discord API.
pub const REQUIRED_SAMPLING_RATE: f32 = 48000.0;

impl<S: AudioSource> ResamplingAudioSource<S> {

    /// Creates a new resampling audio source from the wrapped `source`
    /// providing audio at the sampling rate `src_rate`. The resulting audio
    /// source will have a sampling rate of `dest_rate`. Both in Hz.
    pub fn new(source: S, src_rate: f32, dest_rate: f32)
            -> ResamplingAudioSource<S> {
        ResamplingAudioSource {
            source,
            step: src_rate / dest_rate,
            offset: 2.0,
            previous: Sample::ZERO,
            next: Sample::ZERO
        }
    }

    /// Creates a new resampling audio source that transforms to the
    /// [REQUIRED_SAMPLING_RATE]. `src_rate` specifies the sampling rate of
    /// `source` in Hz.
    pub fn new_to_required(source: S, src_rate: f32)
            -> ResamplingAudioSource<S> {
        Self::new(source, src_rate, REQUIRED_SAMPLING_RATE)
    }
}

impl<S: AudioSource> AudioSource for ResamplingAudioSource<S> {
    fn next(&mut self) -> Option<Sample> {
        while self.offset > 1.0 {
            if let Some(next) = self.source.next() {
                self.previous = self.next;
                self.next = next;
                self.offset -= 1.0;
            }
            else {
                return None;
            }
        }

        let result =
            self.previous * (1.0 - self.offset) + self.next * self.offset;
        self.offset += self.step;
        Some(result)
    }
}
