use rambot_api::audio::{AudioSource, Sample};

use std::collections::HashMap;

/// A mixer manages multiple [AudioSource]s and adds their outputs.
pub struct Mixer<S: AudioSource> {
    layers: HashMap<String, S>
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

    /// Adds a new layer with the given name to this mixer. The provided source
    /// is used for audio input. If there is already a layer wit this name,
    /// this method will panic, as it should have been sorted out before.
    pub fn add_layer(&mut self, name: impl Into<String>, source: S) {
        let name = name.into();

        if self.contains_layer(&name) {
            panic!("Attempted to add duplicate layer.");
        }

        self.layers.insert(name, source);
    }

    /// Removes the layer with the given name and returns whether a layer was
    /// removed, i.e. there was one with the given name.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        self.layers.remove(name).is_some()
    }

    /// Gets a mutable reference to the layer with the given name, if present.
    pub fn layer_mut(&mut self, name: &str) -> Option<&mut S> {
        self.layers.get_mut(name)
    }
}

impl<S: AudioSource> AudioSource for Mixer<S> {
    fn next(&mut self) -> Option<Sample> {
        let samples = self.layers.values_mut()
            .map(S::next)
            .flat_map(Option::into_iter);
        let mut sum = Sample::ZERO;
        let mut some = false;

        for sample in samples {
            some = true;
            sum += sample;
        }

        if some {
            Some(sum)
        }
        else {
            None
        }
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
        let mut mixer = Mixer::new();
        let layer = VecAudioSource::new(Vec::<Sample>::new());

        assert!(!mixer.contains_layer("test-layer-1"));

        mixer.add_layer("test-layer-1", layer.clone());

        assert!(mixer.contains_layer("test-layer-1"));
        assert!(!mixer.contains_layer("test-layer-2"));

        mixer.add_layer("test-layer-2", layer.clone());

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
        mixer.add_layer("1", layer_1);
        mixer.add_layer("2", layer_2);

        assert_eq!(Some((0.0, 1.0).into()), mixer.next());
        assert_eq!(Some((-1.0, 1.0).into()), mixer.next());
        assert_eq!(Some((1.0, 0.0).into()), mixer.next());
        assert_eq!(None, mixer.next());
    }
}