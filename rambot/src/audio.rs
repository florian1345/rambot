use rambot_api::{AudioSource, Sample};

use songbird::input::reader::MediaSource;

use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

struct AudioBuffer {
    data: Vec<Sample>,
    active_len: usize
}

impl AudioBuffer {
    fn inactive_slice(&mut self, len: usize) -> &mut [Sample] {
        &mut self.data[self.active_len..(self.active_len + len)]
    }

    fn extend_capacity(&mut self, target_len: usize) {
        if target_len > self.data.len() {
            self.data.append(
                &mut vec![Sample::ZERO; target_len - self.data.len()]);
        }
    }

    fn remove_first(&mut self, amount: usize) {
        if amount >= self.active_len {
            self.active_len = 0;
            return;
        }

        for i in 0..(self.active_len - amount) {
            self.data[i] = self.data[amount + i];
        }

        self.active_len -= amount;
    }
}

struct Layer {
    name: String,
    source: Option<Box<dyn AudioSource + Send>>,
    buffer: AudioBuffer
}

impl Layer {
    fn active(&self) -> bool {
        self.buffer.active_len > 0 || self.source.is_some()
    }

    fn play(&mut self, source: Box<dyn AudioSource + Send>) {
        self.buffer.active_len = 0;
        self.source = Some(source);
    }

    fn stop(&mut self) -> bool {
        self.buffer.active_len = 0;
        self.source.take().is_some()
    }
}

/// A mixer manages multiple [AudioSource]s and adds their outputs.
pub struct Mixer {
    layers: Vec<Layer>,
    names: HashMap<String, usize>
}

impl Mixer {

    /// Creates a new mixer without layers.
    pub fn new() -> Mixer {
        Mixer {
            layers: Vec::new(),
            names: HashMap::new()
        }
    }

    /// Indicates whether this mixer contains a layer with the given name.
    pub fn contains_layer(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    /// Adds a new layer with the given name to this mixer, which will
    /// initially be inactive. The method returns `true` if and only if the
    /// layer was successfully added, i.e. there was no layer with the same
    /// name before.
    pub fn add_layer(&mut self, name: impl Into<String>) -> bool {
        let name = name.into();

        if self.contains_layer(&name) {
            return false;
        }

        let index = self.layers.len();
        self.layers.push(Layer {
            name: name.clone(),
            source: None,
            buffer: AudioBuffer {
                data: Vec::new(),
                active_len: 0
            }
        });
        self.names.insert(name, index);

        true
    }

    /// Removes the layer with the given name and returns whether a layer was
    /// removed, i.e. there was one with the given name.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        if let Some(index) = self.names.remove(name) {
            self.layers.swap_remove(index);

            if let Some(moved_layer) = self.layers.get(index) {
                *self.names.get_mut(&moved_layer.name).unwrap() = index;
            }

            true
        }
        else {
            false
        }
    }

    /// Indicates whether this mixer is currently active, i.e. there is an
    /// active layer.
    pub fn active(&self) -> bool {
        self.layers.iter().map(|l| &l.source).any(Option::is_some)
    }

    /// Plays the given audio `source` on the `layer` with the given name.
    /// Panics if the layer does not exist.
    pub fn play_on_layer(&mut self, layer: &str,
            source: Box<dyn AudioSource + Send>) {
        let index = *self.names.get(layer).unwrap();
        self.layers.get_mut(index).unwrap().play(source);
    }

    /// Stops the audio source currently played on the `layer` with the given
    /// name. Returns true if and only if there was something playing on the
    /// layer before. Panics if the layer does not exist.
    pub fn stop_layer(&mut self, layer: &str) -> bool {
        let index = *self.names.get(layer).unwrap();
        self.layers.get_mut(index).unwrap().stop()
    }

    /// Stops audio on all layers. Returns true if and only if at there was
    /// audio playing before on at least one layer.
    pub fn stop_all(&mut self) -> bool {
        self.layers.iter_mut()
            .map(Layer::stop)
            .any(|x| x)
    }

    /// Returns an iterator over the names of all layers in this mixer.
    pub fn layers(&self) -> impl Iterator<Item = &String> {
        self.names.keys()
    }
}

impl AudioSource for Mixer {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let mut size = usize::MAX;
        let mut active_layers = Vec::new();

        for layer in self.layers.iter_mut() {
            if layer.active() {
                if layer.buffer.active_len < buf.len() {
                    layer.buffer.extend_capacity(buf.len());
                }

                if let Some(source) = &mut layer.source {
                    let inactive_len = buf.len() - layer.buffer.active_len;
                    let inactive_slice =
                        layer.buffer.inactive_slice(inactive_len);
                    let sample_count = source.read(inactive_slice)?;

                    layer.buffer.active_len += sample_count;

                    if sample_count == 0 {
                        layer.source = None;

                        if layer.buffer.active_len == 0 {
                            continue;
                        }
                    }
                }

                size = size.min(layer.buffer.active_len);
                active_layers.push(layer);
            }
        }

        if size == usize::MAX {
            return Ok(0);
        }

        for i in 0..size {
            let mut sum = Sample::ZERO;

            for layer in active_layers.iter_mut() {
                sum += &layer.buffer.data[i];
            }

            buf[i] = sum;
        }

        for layer in active_layers {
            layer.buffer.remove_first(size);
        }

        Ok(size)
    }
}

/// A wrapper of an [AudioSource] that implements the [Read] trait. It outputs
/// 32-bit floating-point PCM audio data.
pub struct PCMRead<S: AudioSource + Send> {
    source: Arc<Mutex<S>>,
    sample_buf: Vec<Sample>
}

impl<S: AudioSource + Send> PCMRead<S> {

    /// Creates a new PCMRead with the given audio source.
    pub fn new(source: Arc<Mutex<S>>) -> PCMRead<S> {
        PCMRead {
            source,
            sample_buf: Vec::new()
        }
    }
}

const SAMPLE_SIZE: usize = 8;

fn to_bytes(s: &Sample) -> Vec<u8> {
    let lle = s.left.to_le_bytes();
    let rle = s.right.to_le_bytes();
    [lle, rle].concat()
}

impl<S: AudioSource + Send> Read for PCMRead<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let sample_capacity = buf.len() / SAMPLE_SIZE;

        if self.sample_buf.len() < sample_capacity {
            self.sample_buf = vec![Sample {
                left: 0.0,
                right: 0.0
            }; sample_capacity];
        }

        let sample_len = self.source.lock().unwrap()
            .read(&mut self.sample_buf[..sample_capacity])?;

        for i in 0..sample_len {
            let sample = &self.sample_buf[i];
            let bytes = to_bytes(sample);
            let buf_stride = &mut buf[i * SAMPLE_SIZE..];

            for i in 0..SAMPLE_SIZE {
                buf_stride[i] = bytes[i];
            }
        }

        Ok(sample_len * SAMPLE_SIZE)
    }
}

impl<S: AudioSource + Send> Seek for PCMRead<S> {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(ErrorKind::Unsupported, "cannot seek PCM read"))
    }
}

impl<S: AudioSource + Send> MediaSource for PCMRead<S> {
    fn is_seekable(&self) -> bool {
        false
    }

    fn len(&self) -> Option<u64> {
        None
    }
}
