use rambot_api::{AudioSource, Sample, AudioSourceList};

use songbird::input::reader::MediaSource;

use std::collections::HashMap;
use std::fmt::Display;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

#[cfg(feature = "bench")]
use std::time::{Duration, Instant};

use vmcircbuffer::double_mapped_buffer::DoubleMappedBuffer;

use crate::key_value::KeyValueDescriptor;
use crate::plugin::{PluginManager, AudioDescriptorList, ResolveError};

struct AudioBuffer {
    data: DoubleMappedBuffer<Sample>,
    head: usize,
    len: usize
}

impl AudioBuffer {
    fn new() -> AudioBuffer {
        AudioBuffer {
            data: DoubleMappedBuffer::new(0).unwrap(),
            head: 0,
            len: 0
        }
    }

    fn ensure_capacity(&mut self, capacity: usize) {
        if self.data.capacity() < capacity {
            let new_data = DoubleMappedBuffer::new(capacity).unwrap();
            
            unsafe {
                new_data.slice_mut().copy_from_slice(self.get_slice(self.len));
            }

            self.data = new_data;
            self.head = 0;
        }
    }

    unsafe fn get_slice(&self, len: usize) -> &[Sample] {
        &self.data.slice_with_offset(self.head)[..len]
    }

    fn advance_head(&mut self, len: usize) {
        self.head = (self.head + len) % self.data.capacity();
        self.len -= len;
    }

    unsafe fn inactive_slice_mut(&mut self) -> &mut [Sample] {
        let offset = (self.head + self.len) % self.data.capacity();
        let len = self.data.capacity() - self.len;
        &mut self.data.slice_with_offset_mut(offset)[..len]
    }

    fn advance_tail(&mut self, len: usize) {
        self.len += len;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

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

    fn new(name: impl Into<String>) -> Layer {
        Layer {
            name: name.into(),
            source: None,
            list: None,
            buffer: AudioBuffer::new(),
            effects: Vec::new(),
            adapters: Vec::new()
        }
    }

    fn active(&self) -> bool {
        self.buffer.len() > 0 || self.source.is_some()
    }

    fn play(&mut self, source: Box<dyn AudioSource + Send>) {
        self.buffer.clear();
        self.source = Some(source);
    }

    fn stop(&mut self) -> bool {
        self.buffer.clear();
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

struct Layers {
    layers: Vec<Layer>,
    names: HashMap<String, usize>,
}

impl Layers {

    fn new() -> Layers {
        Layers {
            layers: Vec::new(),
            names: HashMap::new()
        }
    }

    fn contains(&self, layer: &str) -> bool {
        self.names.contains_key(layer)
    }

    fn get(&self, layer: &str) -> &Layer {
        let index = *self.names.get(layer).unwrap();
        self.layers.get(index).unwrap()
    }

    fn get_mut(&mut self, layer: &str) -> &mut Layer {
        let index = *self.names.get(layer).unwrap();
        self.layers.get_mut(index).unwrap()
    }

    fn push(&mut self, layer: Layer) {
        self.names.insert(layer.name.clone(), self.layers.len());
        self.layers.push(layer);
    }

    fn remove(&mut self, layer: &str) -> bool {
        if let Some(index) = self.names.remove(layer) {
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

    fn iter(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.layers.iter_mut()
    }
}

/// A mixer manages multiple [AudioSource]s and adds their outputs.
pub struct Mixer {
    layers: Layers,
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
            layers: Layers::new(),
            plugin_manager
        }
    }

    /// Indicates whether this mixer contains a layer with the given name.
    pub fn contains_layer(&self, name: &str) -> bool {
        self.layers.contains(name)
    }

    /// Gets a reference to the layer with the given `name`. Panics if it does
    /// not exist.
    pub fn layer(&self, name: &str) -> &Layer {
        self.layers.get(name)
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

        self.layers.push(Layer::new(name));

        true
    }

    /// Removes the layer with the given name and returns whether a layer was
    /// removed, i.e. there was one with the given name.
    pub fn remove_layer(&mut self, name: &str) -> bool {
        self.layers.remove(name)
    }

    /// Indicates whether this mixer is currently active, i.e. there is an
    /// active layer.
    pub fn active(&self) -> bool {
        self.layers.iter().map(|l| &l.source).any(Option::is_some)
    }

    /// Adds an effect to the layer with the given name. If the effect is
    /// unique, any old version of it will be removed before. If audio is
    /// currently being played on the given layer, any changes in the effect
    /// pipeline will also be applied to that audio.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer to which to apply the effect.
    /// * `descriptor`: A [KeyValueDescriptor] describing the effect to add.
    ///
    /// # Errors
    ///
    /// If audio is currently being played, new effects need to be resolved.
    /// This can cause a [ResolveError].
    pub fn add_effect(&mut self, layer: &str, descriptor: KeyValueDescriptor)
            -> Result<(), ResolveError> {
        let layer = self.layers.get_mut(layer);

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

    /// Clears all effects from the layer with the given name. If audio is
    /// currently being played on the given layer, all effects will be removed
    /// from it.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer from which to remove all effects.
    ///
    /// # Returns
    ///
    /// The number of effects that were removed.
    pub fn clear_effects(&mut self, layer: &str) -> usize {
        let layer = self.layers.get_mut(layer);

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
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer from which to remove effects.
    /// * `predicate`: A function which takes as input a reference to a
    /// [KeyValueDescriptor] representing an effect and decides whether this
    /// effect should be retained (`true`) or not (`false`).
    ///
    /// # Returns
    ///
    /// The number of effects that were removed.
    ///
    /// # Errors
    ///
    /// If a lower-level effect was removed while a higher-level one was
    /// retained, the higher-level effect needs to be re-resolved with audio
    /// that does not have the lower-level effect. This can cause a
    /// [ResolveError].
    pub fn retain_effects<P>(&mut self, layer: &str, mut predicate: P)
        -> Result<usize, ResolveError>
    where
        P: FnMut(&KeyValueDescriptor) -> bool
    {
        let layer = self.layers.get_mut(layer);
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

    /// Adds an adapter to the layer with the given name. If a playlist is
    /// currently being played, it will remain unaffected. The adapter only
    /// takes effect once a new playlist is started.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer to which to apply the adapter.
    /// * `descriptor`: A [KeyValueDescriptor] describing the adapter to add.
    pub fn add_adapter(&mut self, layer: &str,
            descriptor: KeyValueDescriptor) {
        let layer = self.layers.get_mut(layer);

        if self.plugin_manager.is_adapter_unique(&descriptor.name) {
            layer.adapters.retain(|d| &d.name != &descriptor.name);
        }

        layer.adapters.push(descriptor);
    }

    /// Removes all adapters from the layer with the given name. If a playlist
    /// is currently being played, it will remain unaffected. The removal of
    /// adapters only takes effect once a new playlist is started.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer from which to remove all adapters.
    ///
    /// # Returns
    ///
    /// The number of adapters that were removed.
    pub fn clear_adapters(&mut self, layer: &str) -> usize {
        let layer = self.layers.get_mut(layer);
        let old_len = layer.adapters.len();
        layer.adapters.clear();
        old_len
    }

    /// Removes all adapters from the `layer` with the given name that do not
    /// match the given `predicate`. If a playlist is currently being played,
    /// it will remain unaffected. The removal of adapters only takes effect
    /// once a new playlist is started.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer from which to remove adapters.
    /// * `predicate`: A function which takes as input a reference to a
    /// [KeyValueDescriptor] representing an adapter and decides whether this
    /// adapter should be retained (`true`) or not (`false`).
    ///
    /// # Returns
    ///
    /// The number of adapters that were removed.
    pub fn retain_adapters<P>(&mut self, layer: &str, predicate: P) -> usize
    where
        P: FnMut(&KeyValueDescriptor) -> bool
    {
        let layer = self.layers.get_mut(layer);
        let old_len = layer.adapters.len();
        layer.adapters.retain(predicate);
        old_len - layer.adapters.len()
    }

    /// Plays audio given some `descriptor` on the `layer` with the given name.
    /// Panics if the layer does not exist.
    pub fn play_on_layer(&mut self, layer: &str, descriptor: &str)
            -> Result<(), io::Error> {
        let layer = self.layers.get_mut(layer);
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
        let layer = self.layers.get_mut(layer);

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
        let layer = self.layers.get_mut(layer);
        layer.stop()
    }

    /// Stops audio on all layers. Returns true if and only if at there was
    /// audio playing before on at least one layer.
    pub fn stop_all(&mut self) -> bool {
        self.layers.iter_mut()
            .map(Layer::stop)
            .collect::<Vec<_>>() // Avoid short circuiting
            .into_iter()
            .any(|x| x)
    }

    /// Returns a slice of all layers in this mixer.
    pub fn layers(&self) -> &[Layer] {
        &self.layers.layers
    }
}

impl AudioSource for Mixer {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        // TODO this is too complex. simplify or divide.

        let mut size = usize::MAX;
        let mut active_layers = Vec::new();

        for layer in self.layers.iter_mut() {
            if !layer.active() {
                continue;
            }

            if layer.buffer.len() < buf.len() {
                layer.buffer.ensure_capacity(buf.len());

                while let Some(source) = &mut layer.source {
                    let sample_count = unsafe {
                        let inactive_slice =
                            layer.buffer.inactive_slice_mut();
                        let count = source.read(inactive_slice)?;
                        layer.buffer.advance_tail(count);
                        count
                    };

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
                    }
                    else {
                        break;
                    }
                }
            }

            // The layer may have been deactivated just now, so we check again

            if !layer.active() {
                continue;
            }

            size = size.min(layer.buffer.len());
            active_layers.push(layer);
        }

        if size == usize::MAX {
            return Ok(0);
        }

        size = size.min(buf.len());

        let mut active_layers = active_layers.into_iter();
        let first_layer = active_layers.next().unwrap();

        unsafe {
            buf[..size].copy_from_slice(first_layer.buffer.get_slice(size));
            first_layer.buffer.advance_head(size);
        }

        for layer in active_layers {
            let slice = unsafe { layer.buffer.get_slice(size) };

            for (i, sample) in slice.iter().enumerate() {
                buf[i] += sample;
            }

            layer.buffer.advance_head(size);
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
    sample_buf: Vec<Sample>,

    #[cfg(feature = "bench")]
    bench_sample_count: usize,

    #[cfg(feature = "bench")]
    bench_duration: Duration
}

impl<S: AudioSource + Send> PCMRead<S> {

    /// Creates a new PCMRead with the given audio source.
    pub fn new(source: Arc<Mutex<S>>) -> PCMRead<S> {
        PCMRead {
            source,
            sample_buf: Vec::new(),

            #[cfg(feature = "bench")]
            bench_sample_count: 0,

            #[cfg(feature = "bench")]
            bench_duration: Duration::ZERO
        }
    }
}

const SAMPLE_SIZE: usize = 8;

#[cfg(feature = "bench")]
const SAMPLES_FOR_REPORT: usize = 96000;

fn to_bytes(buf: &mut [u8], s: &Sample) {
    buf[..4].copy_from_slice(&s.left.to_le_bytes());
    buf[4..8].copy_from_slice(&s.right.to_le_bytes());
}

impl<S: AudioSource + Send> Read for PCMRead<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(feature = "bench")]
        let before = Instant::now();

        let sample_capacity = buf.len() / SAMPLE_SIZE;

        if self.sample_buf.len() < sample_capacity {
            self.sample_buf = vec![Sample::ZERO; sample_capacity];
        }

        let sample_len = self.source.lock().unwrap()
            .read(&mut self.sample_buf[..sample_capacity])?;
        let mut buf_stride = buf;

        for i in 0..sample_len {
            let sample = &self.sample_buf[i];
            to_bytes(buf_stride, sample);
            buf_stride = &mut buf_stride[SAMPLE_SIZE..];
        }

        #[cfg(feature = "bench")]
        {
            let after = Instant::now();
            self.bench_sample_count += sample_len;
            self.bench_duration += after - before;

            if self.bench_sample_count >= SAMPLES_FOR_REPORT {
                let nanos_per_sample = self.bench_duration.as_nanos() as f64 /
                    self.bench_sample_count as f64;

                self.bench_sample_count = 0;
                self.bench_duration = Duration::ZERO;

                log::info!("Measured average of {:.02} ns per sample.",
                    nanos_per_sample);
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

#[cfg(test)]
mod tests {

    use super::*;

    struct MockAudioSource {
        samples: Vec<Sample>,
        index: usize,
        segment_size: usize
    }

    impl MockAudioSource {
        fn new(samples: Vec<Sample>) -> MockAudioSource {
            MockAudioSource::with_segment_size(samples, usize::MAX)
        }
    
        fn with_segment_size(samples: Vec<Sample>, segment_size: usize)
                -> MockAudioSource {
            MockAudioSource {
                samples,
                index: 0,
                segment_size
            }
        }
    }

    impl AudioSource for MockAudioSource {
        fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
            let remaining = &self.samples[self.index..];
            let len = buf.len().min(remaining.len()).min(self.segment_size);
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

    fn pcm_read_to_end<S>(mut buf: &mut [u8], read: &mut PCMRead<S>) -> usize
    where
        S: AudioSource + Send
    {
        let mut total = 0;

        loop {
            let count = read.read(buf).unwrap();

            if count == 0 {
                return total;
            }

            buf = &mut buf[count..];
            total += count;

            if buf.len() == 0 {
                return total;
            }
        }
    }

    #[test]
    fn pcm_read_zeros() {
        let source = MockAudioSource::new(vec![Sample::ZERO; 100]);
        let mut pcm_read = PCMRead::new(Arc::new(Mutex::new(source)));
        let mut buf = vec![1; 1024];

        assert_eq!(800, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().enumerate()
            .all(|(i, b)| (i < 800 && b == 0) || (i >= 800 && b == 1)));
    }

    #[test]
    fn pcm_read_zeros_split() {
        let source = MockAudioSource::new(vec![Sample::ZERO; 100]);
        let mut pcm_read = PCMRead::new(Arc::new(Mutex::new(source)));
        let mut buf = vec![1; 256];

        assert_eq!(256, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().all(|b| b == 0));
        
        let mut buf = vec![1; 1024];

        assert_eq!(544, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().enumerate()
            .all(|(i, b)| (i < 544 && b == 0) || (i >= 544 && b == 1)));
    }

    fn mixer_read_to_end(mut buf: &mut [Sample], mixer: &mut Mixer) -> usize {
        let mut total = 0;

        loop {
            let count = mixer.read(buf).unwrap();

            if count == 0 {
                return total;
            }

            buf = &mut buf[count..];
            total += count;

            if buf.len() == 0 {
                return total;
            }
        }
    }

    fn test_audio_1() -> Vec<Sample> {
        let mut result = Vec::with_capacity(64);

        for i in 0..64 {
            let x = i as f32;
            let left = x + 1.0;
            let right = 2.0 * x;

            result.push(Sample {
                left,
                right
            })
        }

        result
    }

    fn test_audio_2() -> Vec<Sample> {
        let mut result = Vec::with_capacity(96);

        for i in 0..96 {
            let x = i as f32;
            let left = 3.0 * x;
            let right = x + 2.0;

            result.push(Sample {
                left,
                right
            })
        }

        result
    }

    fn test_audio_sum() -> Vec<Sample> {
        let audio_1 = test_audio_1();
        let audio_2 = test_audio_2();
        let mut sum = Vec::with_capacity(96);

        for i in 0..64 {
            sum.push(audio_1[i] + audio_2[i]);
        }

        for i in 64..96 {
            sum.push(audio_2[i]);
        }

        sum
    }

    fn assert_approximately_equal(expected: &[Sample], actual: &[Sample]) {
        const EPS: f32 = 0.001;

        assert_eq!(expected.len(), actual.len());

        let zipped = expected.iter().cloned().zip(actual.iter().cloned());

        for (expected, actual) in zipped {
            assert!((expected.left - actual.left).abs() < EPS);
            assert!((expected.right - actual.right).abs() < EPS);
        }
    }

    fn add_layer(mixer: &mut Mixer, name: &str, samples: Vec<Sample>,
            segment_size: Option<usize>) {
        mixer.layers.push(Layer::new(name));

        let audio = if let Some(segment_size) = segment_size {
            MockAudioSource::with_segment_size(samples, segment_size)
        }
        else {
            MockAudioSource::new(samples)
        };

        mixer.layers.get_mut(name).source = Some(Box::new(audio));
    }

    #[test]
    fn mixer_single_audio_source() {
        let mut mixer = Mixer::new(Arc::new(PluginManager::mock()));
        add_layer(&mut mixer, "test", test_audio_1(), None);
        let mut buf = vec![Sample::ZERO; 100];

        assert_eq!(64, mixer_read_to_end(&mut buf, &mut mixer));
        assert_approximately_equal(&test_audio_1(), &buf[..64]);
    }

    #[test]
    fn mixer_two_audio_sources() {
        let mut mixer = Mixer::new(Arc::new(PluginManager::mock()));
        add_layer(&mut mixer, "test1", test_audio_1(), None);
        add_layer(&mut mixer, "test2", test_audio_2(), None);
        let mut buf = vec![Sample::ZERO; 100];

        assert_eq!(96, mixer_read_to_end(&mut buf, &mut mixer));
        assert_approximately_equal(&test_audio_sum(), &buf[..96]);
    }

    #[test]
    fn mixer_two_segmented_audio_sources() {
        let mut mixer = Mixer::new(Arc::new(PluginManager::mock()));
        add_layer(&mut mixer, "test1", test_audio_1(), Some(5));
        add_layer(&mut mixer, "test2", test_audio_2(), Some(7));
        let mut buf = vec![Sample::ZERO; 100];

        assert_eq!(96, mixer_read_to_end(&mut buf, &mut mixer));
        assert_approximately_equal(&test_audio_sum(), &buf[..96]);
    }
}
