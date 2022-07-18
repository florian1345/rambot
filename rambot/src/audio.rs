use rambot_api::{AudioSource, Sample, AudioSourceList};

use songbird::input::reader::MediaSource;

use std::collections::HashMap;
use std::fmt::Display;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use crate::key_value::KeyValueDescriptor;
use crate::plugin::{PluginManager, AudioDescriptorList, ResolveError};

struct SingleAudioSourceList {
    descriptor: Option<String>
}

impl SingleAudioSourceList {
    fn new(descriptor: String) -> SingleAudioSourceList {
        SingleAudioSourceList {
            descriptor: Some(descriptor)
        }
    }
}

impl AudioSourceList for SingleAudioSourceList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        Ok(self.descriptor.take())
    }
}

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

/// A single layer of a [Mixer] which wraps up to one active [AudioSource]. The
/// public methods of this type only allow access to the general information
/// about the structure of this layer, not the actual audio played.
pub struct Layer {
    name: String,
    source: Option<Box<dyn AudioSource + Send>>,
    list: Option<Box<dyn AudioSourceList + Send>>,
    buffer: AudioBuffer,
    effects: Vec<KeyValueDescriptor>,
    adapters: Vec<KeyValueDescriptor>
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
        self.list.take().is_some() | self.source.take().is_some()
    }

    /// Gets the name of this layer.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets a slice of [KeyValueDescriptor]s representing the audio effects
    /// that are active on this layer. The order in the slice is equal to the
    /// order in which they are applied to the audio.
    pub fn effects(&self) -> &[KeyValueDescriptor] {
        &self.effects
    }

    /// Gets a slice of [KeyValueDescriptor]s representing the adapters that
    /// are active on this layer. The order in the slice is equal to the order
    /// in which they are applied to the playlist.
    pub fn adapters(&self) -> &[KeyValueDescriptor] {
        &self.adapters
    }
}

/// A mixer manages multiple [AudioSource]s and adds their outputs.
pub struct Mixer {
    layers: Vec<Layer>,
    names: HashMap<String, usize>,
    plugin_manager: Arc<PluginManager>
}

fn to_io_err<T, E: Display>(r: Result<T, E>) -> Result<T, io::Error> {
    r.map_err(|e| io::Error::new(ErrorKind::Other, format!("{}", e)))
}

fn play_source_on_layer<P>(layer: &mut Layer,
    mut source: Box<dyn AudioSource + Send>, plugin_manager: &P)
    -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    for effect in &layer.effects {
        source = to_io_err(plugin_manager.as_ref()
            .resolve_effect(&effect.name, &effect.key_values, source))?;
    }

    layer.play(source);
    Ok(())
}

fn play_on_layer<P>(layer: &mut Layer, descriptor: &str, plugin_manager: &P)
    -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    let source =
        to_io_err(plugin_manager.as_ref().resolve_audio_source(descriptor))?;

    play_source_on_layer(layer, source, plugin_manager)
}

fn play_list_on_layer<P>(layer: &mut Layer,
    mut list: Box<dyn AudioSourceList + Send>, plugin_manager: &P)
    -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    for adapter in &layer.adapters {
        list = to_io_err(plugin_manager.as_ref().resolve_adapter(
            &adapter.name, &adapter.key_values, list))?;
    }

    if let Some(descriptor) = list.next()? {
        play_on_layer(layer, &descriptor, plugin_manager)?;
        layer.list = Some(list);
    }

    Ok(())
}

fn reapply_effects_after_removal<P>(layer: &mut Layer,
    first_removed_idx: usize, total_removed: usize, plugin_manager: &P)
    -> Result<(), ResolveError>
where
    P: AsRef<PluginManager>
{
    if let Some(mut source) = layer.source.take() {
        // TODO find a way to recover the audio source if this fails

        let old_len = layer.effects.len() + total_removed;

        for _ in 0..(old_len - first_removed_idx) {
            source = source.take_child();
        }

        for old_effect in &layer.effects[first_removed_idx..] {
            source = plugin_manager.as_ref().resolve_effect(
                &old_effect.name, &old_effect.key_values, source)?;
        }

        layer.source = Some(source);
    }

    Ok(())
}

impl Mixer {

    /// Creates a new mixer without layers.
    pub fn new(plugin_manager: Arc<PluginManager>) -> Mixer {
        Mixer {
            layers: Vec::new(),
            names: HashMap::new(),
            plugin_manager
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
            list: None,
            buffer: AudioBuffer {
                data: Vec::new(),
                active_len: 0
            },
            effects: Vec::new(),
            adapters: Vec::new()
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

    pub fn layer(&self, layer: &str) -> &Layer {
        let index = *self.names.get(layer).unwrap();
        self.layers.get(index).unwrap()
    }

    fn layer_mut(&mut self, layer: &str) -> &mut Layer {
        let index = *self.names.get(layer).unwrap();
        self.layers.get_mut(index).unwrap()
    }

    pub fn add_effect(&mut self, layer: &str, descriptor: KeyValueDescriptor)
            -> Result<(), ResolveError> {
        // TODO convince the borrow checker that it is ok to use layer_mut

        let index = *self.names.get(layer).unwrap();
        let layer = self.layers.get_mut(index).unwrap();

        if self.plugin_manager.is_effect_unique(&descriptor.name) {
            // We need to remove the old effect of the same name

            let removed_idx = layer.effects.iter().enumerate()
                .find(|(_, e)| &e.name == &descriptor.name)
                .map(|(i, _)| i);

            if let Some(idx) = removed_idx {
                layer.effects.remove(idx);
                reapply_effects_after_removal(
                    layer, idx, 1, &self.plugin_manager)?;
            }
        }

        if let Some(source) = layer.source.take() {
            // TODO find a way to recover the audio source if this fails
            
            layer.source = Some(self.plugin_manager.resolve_effect(
                &descriptor.name, &descriptor.key_values, source)?);
        }

        layer.effects.push(descriptor);
        Ok(())
    }

    pub fn clear_effects(&mut self, layer: &str) -> usize {
        let layer = self.layer_mut(layer);

        if let Some(mut source) = layer.source.take() {
            while source.has_child() {
                source = source.take_child();
            }

            layer.source = Some(source);
        }

        let old_len = layer.effects.len();
        layer.effects.clear();
        old_len
    }

    /// Removes all effects from the `layer` with the given name that do not
    /// match the given `predicate`.
    pub fn retain_effects<P>(&mut self, layer: &str, mut predicate: P)
        -> Result<usize, ResolveError>
    where
        P: FnMut(&KeyValueDescriptor) -> bool
    {
        // TODO convince the borrow checker that it is ok to use layer_mut

        let index = *self.names.get(layer).unwrap();
        let layer = self.layers.get_mut(index).unwrap();

        let mut index = 0;
        let mut first_removed_idx = None;
        let old_len = layer.effects.len();

        layer.effects.retain(|descriptor| {
            if predicate(descriptor) {
                index += 1;
                true
            }
            else {
                first_removed_idx.get_or_insert(index);
                false
            }
        });

        let total_removed = old_len - layer.effects.len();

        if let Some(first_removed_idx) = first_removed_idx {
            reapply_effects_after_removal(layer, first_removed_idx,
                total_removed, &self.plugin_manager)?;
        }

        Ok(total_removed)
    }

    pub fn add_adapter(&mut self, layer: &str,
            descriptor: KeyValueDescriptor) {
        self.layer_mut(layer).adapters.push(descriptor);
    }

    pub fn clear_adapters(&mut self, layer: &str) -> usize {
        let layer = self.layer_mut(layer);
        let old_len = layer.adapters.len();
        layer.adapters.clear();
        old_len
    }

    /// Removes all adapters from the `layer` with the given name that do not
    /// match the given `predicate`.
    pub fn retain_adapters<P>(&mut self, layer: &str, predicate: P) -> usize
    where
        P: FnMut(&KeyValueDescriptor) -> bool
    {
        let layer = self.layer_mut(layer);
        let old_len = layer.adapters.len();
        layer.adapters.retain(predicate);
        old_len - layer.adapters.len()
    }

    /// Plays audio given some `descriptor` on the `layer` with the given name.
    /// Panics if the layer does not exist.
    pub fn play_on_layer(&mut self, layer: &str, descriptor: &str)
            -> Result<(), io::Error> {

        // TODO convince the borrow checker that it is ok to use layer_mut

        let index = *self.names.get(layer).unwrap();
        let layer = self.layers.get_mut(index).unwrap();
        let audio = to_io_err(self.plugin_manager.resolve_audio_descriptor_list(descriptor))?;

        layer.stop();

        match audio {
            AudioDescriptorList::Single(source) => {
                play_list_on_layer(layer,
                    Box::new(SingleAudioSourceList::new(source)),
                    &self.plugin_manager)?
            },
            AudioDescriptorList::List(list) => {
                play_list_on_layer(layer, list, &self.plugin_manager)?
            }
        }

        Ok(())
    }

    /// Skips to the next audio source provided by the list on the layer with
    /// the given name. If querying the next piece or initiating playback
    /// fails, an appropriate error is returned.
    pub fn skip_on_layer(&mut self, layer: &str) -> Result<(), io::Error> {

        // TODO convince the borrow checker that it is ok to use layer_mut

        let index = *self.names.get(layer).unwrap();
        let layer = self.layers.get_mut(index).unwrap();

        match layer.list.as_mut().map(|l| l.next()) {
            Some(Ok(Some(next))) => {
                play_on_layer(layer, &next, &self.plugin_manager)?;
                Ok(())
            },
            Some(Err(e)) => Err(e),
            Some(Ok(None)) | None => {
                layer.stop();
                Ok(())
            }
        }
    }

    /// Stops the audio source currently played on the `layer` with the given
    /// name. Returns true if and only if there was something playing on the
    /// layer before. Panics if the layer does not exist.
    pub fn stop_layer(&mut self, layer: &str) -> bool {
        let layer = self.layer_mut(layer);
        layer.stop()
    }

    /// Stops audio on all layers. Returns true if and only if at there was
    /// audio playing before on at least one layer.
    pub fn stop_all(&mut self) -> bool {
        self.layers.iter_mut()
            .map(Layer::stop)
            .any(|x| x)
    }

    /// Returns a slice of all layers in this mixer.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
}

impl AudioSource for Mixer {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        // TODO this is too complex. simplify or divide.

        let mut size = usize::MAX;
        let mut active_layers = Vec::new();

        'outer:
        for layer in self.layers.iter_mut() {
            if layer.active() {
                if layer.buffer.active_len < buf.len() {
                    layer.buffer.extend_capacity(buf.len());
                }

                while let Some(source) = &mut layer.source {
                    let inactive_len = buf.len() - layer.buffer.active_len;
                    let inactive_slice =
                        layer.buffer.inactive_slice(inactive_len);
                    let sample_count = source.read(inactive_slice)?;

                    layer.buffer.active_len += sample_count;

                    if sample_count == 0 {
                        if let Some(list) = &mut layer.list {
                            if let Some(next) = list.next()? {
                                // Audio source ran out but list continues

                                play_on_layer(
                                    layer, &next, &self.plugin_manager)?;
                            }
                            else {
                                // Audio source ran out list is finished

                                layer.list = None;
                                layer.source = None;
                            }
                        }
                        else {
                            // Audio source ran out and there is no list
    
                            layer.source = None;
                        }

                        if layer.source.is_none() &&
                                layer.buffer.active_len == 0 {
                            // Inactive layer

                            continue 'outer;
                        }
                    }
                    else {
                        break;
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

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("mixer has no child")
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
