use rambot_api::audio::{AudioSource, Sample};

use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::{Arc, Mutex};

/// A mixer manages multiple [AudioSource]s and adds their outputs.
pub struct Mixer<S: AudioSource> {
    layers: HashMap<String, Option<S>>
}

impl<S: AudioSource> Mixer<S> {

    /// Creates a new mixer without layers.
    pub fn new() -> Mixer<S> {
        Mixer {
            layers: HashMap::new()
        }
    }

    /// Indicates whether this mixer contains a layer with the given name.
    pub fn contains_layer(&self, name: &str) -> bool {
        self.layers.contains_key(name)
    }

    /// Adds a new layer with the given name to this mixer, which will
    /// initially be inactive. If there is already a layer with this name, this
    /// method will panic, as it should have been sorted out before.
    pub fn add_layer(&mut self, name: impl Into<String>) {
        let name = name.into();

        if self.contains_layer(&name) {
            panic!("Attempted to add duplicate layer.");
        }

        self.layers.insert(name, None);
    }

    /// Removes the layer with the given name and returns whether a layer was
    /// removed, i.e. there was one with the given name.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        self.layers.remove(name).is_some()
    }

    /// Indicates whether this mixer is currently active, i.e. there is an
    /// active layer.
    pub fn active(&self) -> bool {
        self.layers.values().any(Option::is_some)
    }

    /// Plays the given audio `source` on the `layer` with the given name.
    /// Panics if the layer does not exist.
    pub fn play_on_layer(&mut self, layer: &str, source: S) {
        *self.layers.get_mut(layer).unwrap() = Some(source);
    }

    /// Stops the audio source currently played on the `layer` with the given
    /// name. Returns true if and only if there was something playing on the
    /// layer before. Panics if the layer does not exist.
    pub fn stop_layer(&mut self, layer: &str) -> bool {
        self.layers.get_mut(layer).unwrap().take().is_some()
    }

    /// Returns an iterator over the names of all layers in this mixer.
    pub fn layers(&self) -> impl Iterator<Item = &String> {
        self.layers.keys()
    }
}

impl<S: AudioSource> AudioSource for Mixer<S> {
    fn next(&mut self) -> Option<Sample> {
        let mut sum = Sample::ZERO;
        let mut some = false;

        for source_opt in self.layers.values_mut() {
            if let Some(source) = source_opt {
                if let Some(sample) = source.next() {
                    some = true;
                    sum += sample;
                }
                else {
                    *source_opt = None;
                }
            }
        }

        if some {
            Some(sum)
        }
        else {
            None
        }
    }
}

/// A wrapper of an [AudioSource] that implements the [Read] trait. It outputs
/// 32-bit floating-point PCM audio data.
pub struct PCMRead<S: AudioSource + Send> {
    source: Arc<Mutex<S>>
}

impl<S: AudioSource + Send> PCMRead<S> {

    /// Creates a new PCMRead with the given audio source.
    pub fn new(source: Arc<Mutex<S>>) -> PCMRead<S> {
        PCMRead { source }
    }
}

const SAMPLE_SIZE: usize = 8;

fn to_bytes(s: Sample) -> Vec<u8> {
    let lle = s.left.to_le_bytes();
    let rle = s.right.to_le_bytes();
    [lle, rle].concat()
}

impl<S: AudioSource + Send> Read for PCMRead<S> {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let mut written_bytes = 0usize;
        let mut source = self.source.lock().unwrap();

        while buf.len() >= SAMPLE_SIZE {
            if let Some(s) = source.next() {
                for (i, &byte) in to_bytes(s).iter().enumerate() {
                    buf[i] = byte;
                }

                buf = &mut buf[SAMPLE_SIZE..];
                written_bytes += SAMPLE_SIZE;
            }
            else {
                break;
            }
        }

        Ok(written_bytes)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use std::vec::IntoIter;

    #[derive(Clone)]
    struct VecAudioSource {
        iterator: IntoIter<Sample>
    }

    impl VecAudioSource {
        fn new(vec: Vec<impl Into<Sample>>) -> VecAudioSource {
            VecAudioSource {
                iterator: vec
                    .into_iter()
                    .map(|s| s.into())
                    .collect::<Vec<_>>()
                    .into_iter()
            }
        }
    }

    impl AudioSource for VecAudioSource {
        fn next(&mut self) -> Option<Sample> {
            self.iterator.next()
        }
    }

    #[test]
    fn mixer_layer_management() {
        let mut mixer = Mixer::<VecAudioSource>::new();

        assert!(!mixer.contains_layer("test-layer-1"));

        mixer.add_layer("test-layer-1");

        assert!(mixer.contains_layer("test-layer-1"));
        assert!(!mixer.contains_layer("test-layer-2"));

        mixer.add_layer("test-layer-2");

        assert!(mixer.contains_layer("test-layer-1"));
        assert!(mixer.contains_layer("test-layer-2"));

        assert!(mixer.remove_layer("test-layer-1"));

        assert!(!mixer.contains_layer("test-layer-1"));
        assert!(mixer.contains_layer("test-layer-2"));

        assert!(!mixer.remove_layer("test-layer-1"));
    }

    #[test]
    fn mixer_mixing() {
        let mut mixer = Mixer::new();
        let layer_1 = VecAudioSource::new(vec![
            (1.0, 1.0),
            (0.0, 1.0),
            (1.0, 0.0)
        ]);
        let layer_2 = VecAudioSource::new(vec![
            (-1.0, 0.0),
            (-1.0, 0.0)
        ]);
        mixer.add_layer("1");
        mixer.add_layer("2");

        mixer.play_on_layer("1", layer_1);
        mixer.play_on_layer("2", layer_2);

        assert_eq!(Some((0.0, 1.0).into()), mixer.next());
        assert_eq!(Some((-1.0, 1.0).into()), mixer.next());
        assert_eq!(Some((1.0, 0.0).into()), mixer.next());
        assert_eq!(None, mixer.next());
    }
}
