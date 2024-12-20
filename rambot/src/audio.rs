use rambot_api::{
    AudioMetadata,
    AudioSource,
    AudioSourceList,
    PluginGuildConfig,
    Sample, SampleDuration, SeekError
};

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::sync::{Arc, RwLock};

#[cfg(feature = "bench")]
use std::time::{Duration, Instant};
use songbird::input::core::io::MediaSource;
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
                new_data.slice_mut()[..self.len]
                    .copy_from_slice(self.get_slice(self.len));
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

type ErrorCallback = Box<dyn Fn(String, io::Error) + Send + Sync>;

fn no_callback() -> ErrorCallback {
    Box::new(|_, _| { })
}

/// A single layer of a [Mixer] which wraps up to one active [AudioSource]. The
/// public methods of this type only allow access to the general information
/// about the structure of this layer, not the actual audio played.
pub struct Layer {
    name: String,
    source: Option<Box<dyn AudioSource + Send + Sync>>,
    list: Option<Box<dyn AudioSourceList + Send + Sync>>,
    error_callback: ErrorCallback,
    buffer: AudioBuffer,
    effects: Vec<KeyValueDescriptor>,
    adapters: Vec<KeyValueDescriptor>,
    plugin_guild_config: PluginGuildConfig
}

impl Layer {

    fn new(name: impl Into<String>) -> Layer {
        Layer {
            name: name.into(),
            source: None,
            list: None,
            error_callback: no_callback(),
            buffer: AudioBuffer::new(),
            effects: Vec::new(),
            adapters: Vec::new(),
            plugin_guild_config: PluginGuildConfig::default()
        }
    }

    fn active(&self) -> bool {
        self.buffer.len() > 0 || self.source.is_some()
    }

    fn set_source(&mut self, source: Box<dyn AudioSource + Send + Sync>) {
        self.source = Some(source);
    }

    fn stop(&mut self) -> bool {
        self.error_callback = no_callback();
        self.buffer.clear();
        self.list.take().is_some() | self.source.take().is_some()
    }

    /// Like `stop`, but does not clear the buffer.
    fn soft_stop(&mut self) {
        self.list = None;
        self.source = None;
        self.error_callback = no_callback();
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

    fn read_from_source<P>(&mut self, capacity: usize, plugin_manager: &P)
        -> Result<(), io::Error>
    where
        P: AsRef<PluginManager>
    {
        self.buffer.ensure_capacity(capacity);

        while let Some(source) = &mut self.source {
            let sample_count = unsafe {
                let inactive_slice = self.buffer.inactive_slice_mut();
                let count = source.read(inactive_slice)?;
                self.buffer.advance_tail(count);
                count
            };

            if sample_count == 0 {
                if let Some(list) = &mut self.list {
                    if let Some(next) = list.next()? {
                        // Audio source ran out but list continues

                        let res = play_on_layer::<false, _>(
                            self, &next, plugin_manager);

                        if let Err(e) = res {
                            (self.error_callback)(self.name.clone(), e);
                            self.soft_stop();
                            break;
                        }
                    }
                    else {
                        // Audio source ran out and list is finished

                        self.soft_stop();
                    }
                }
                else {
                    // Audio source ran out and there is no list

                    self.soft_stop();
                }
            }
            else {
                break;
            }
        }

        Ok(())
    }
}

struct Layers {
    layers: Vec<Layer>,
    names: HashMap<String, usize>
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

// TODO errors for every layer-dependent Mixer method, rely on errors to report
// missing layers

/// An enumeration of the different errors that can occur when calling
/// [Mixer::layer_metadata].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayerMetadataError {

    /// Metadata about a layer which does not exist was queried. The name of
    /// the layer is wrapped.
    LayerDoesNotExist(String),

    /// Metadata about a layer which does exist but currently plays no audio
    /// was queried. The name of the layer is wrapped.
    LayerNotActive(String)
}

impl Display for LayerMetadataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LayerMetadataError::LayerDoesNotExist(layer) =>
                write!(f, "Found no layer with name `{}`.", layer),
            LayerMetadataError::LayerNotActive(layer) =>
                write!(f, "No audio is being played on layer `{}`.", layer)
        }
    }
}

impl Error for LayerMetadataError { }

/// An enumeration of the different errors that can occur when calling
/// [Mixer::seek_on_layer].
#[derive(Debug)]
pub enum SeekOnLayerError {

    /// The user requested to seek on a layer that is currently not playing any
    /// audio.
    LayerNotActive(String),

    /// The audio source that is currently being played on the layer where the
    /// user attempted to seek raised an error in [AudioSource::seek].
    SeekError(SeekError)
}

impl Display for SeekOnLayerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SeekOnLayerError::LayerNotActive(l) =>
                write!(f, "No audio is being played on layer `{}`.", l),
            SeekOnLayerError::SeekError(e) => write!(f, "{}", e)
        }
    }
}

impl Error for SeekOnLayerError { }

impl From<SeekError> for SeekOnLayerError {
    fn from(e: SeekError) -> SeekOnLayerError {
        SeekOnLayerError::SeekError(e)
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

fn play_source_on_layer<const CLEAR_BUF: bool, P>(layer: &mut Layer,
    mut source: Box<dyn AudioSource + Send + Sync>, plugin_manager: &P)
    -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    for effect in &layer.effects {
        source = to_io_err(plugin_manager.as_ref()
            .resolve_effect(&effect.name, &effect.key_values, source,
                &layer.plugin_guild_config)
            .map_err(|(e, _)| e))?;
    }

    if CLEAR_BUF {
        layer.buffer.clear();
    }

    layer.set_source(source);
    Ok(())
}

fn play_on_layer<const CLEAR_BUF: bool, P>(layer: &mut Layer, descriptor: &str,
    plugin_manager: &P) -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    let source = to_io_err(
        plugin_manager.as_ref().resolve_audio_source(
            descriptor, &layer.plugin_guild_config))?;

    play_source_on_layer::<CLEAR_BUF, _>(layer, source, plugin_manager)
}

fn play_list_on_layer<const CLEAR_BUF: bool, P>(layer: &mut Layer,
    mut list: Box<dyn AudioSourceList + Send + Sync>, plugin_manager: &P)
    -> Result<(), io::Error>
where
    P: AsRef<PluginManager>
{
    for adapter in &layer.adapters {
        list = to_io_err(plugin_manager.as_ref().resolve_adapter(
            &adapter.name, &adapter.key_values, list,
            &layer.plugin_guild_config))?;
    }

    if let Some(descriptor) = list.next()? {
        play_on_layer::<CLEAR_BUF, _>(layer, &descriptor, plugin_manager)?;
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
    let mut result = Ok(());

    if let Some(mut source) = layer.source.take() {
        let old_len = layer.effects.len() + total_removed;

        for _ in 0..(old_len - first_removed_idx) {
            source = source.take_child();
        }

        for old_effect in &layer.effects[first_removed_idx..] {
            let effect_res = plugin_manager.as_ref().resolve_effect(
                &old_effect.name, &old_effect.key_values, source,
                &layer.plugin_guild_config);

            match effect_res {
                Ok(effect) => source = effect,
                Err((err, child)) => {
                    result = Err(err);
                    source = child;
                }
            }
        }

        layer.source = Some(source);
    }

    result
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
                .find(|(_, e)| e.name == descriptor.name)
                .map(|(i, _)| i);

            if let Some(idx) = removed_idx {
                layer.effects.remove(idx);
                reapply_effects_after_removal(
                    layer, idx, 1, &self.plugin_manager)?;
            }
        }

        if let Some(source) = layer.source.take() {
            let effect_res = self.plugin_manager.resolve_effect(
                &descriptor.name, &descriptor.key_values, source,
                &layer.plugin_guild_config);

            match effect_res {
                Ok(effect) => layer.source = Some(effect),
                Err((err, child)) => {
                    layer.source = Some(child);
                    return Err(err);
                }
            }
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
    ///   [KeyValueDescriptor] representing an effect and decides whether this
    ///   effect should be retained (`true`) or not (`false`).
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
            layer.adapters.retain(|d| d.name != descriptor.name);
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
    ///   [KeyValueDescriptor] representing an adapter and decides whether this
    ///   adapter should be retained (`true`) or not (`false`).
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
    pub fn play_on_layer<E>(&mut self, layer: &str, descriptor: &str,
        plugin_guild_config: PluginGuildConfig, error_callback: E)
        -> Result<(), io::Error>
    where
        E: Fn(String, io::Error) + Send + Sync + 'static
    {
        let layer = self.layers.get_mut(layer);
        let audio = to_io_err(
            self.plugin_manager.resolve_audio_descriptor_list(descriptor,
                &layer.plugin_guild_config))?;

        layer.stop();
        layer.plugin_guild_config = plugin_guild_config;
        layer.error_callback = Box::new(error_callback);

        match audio {
            AudioDescriptorList::Single(source) => {
                play_list_on_layer::<true, _>(layer,
                    Box::new(SingleAudioSourceList::new(source)),
                    &self.plugin_manager)?
            },
            AudioDescriptorList::List(list) => {
                play_list_on_layer::<true, _>(
                    layer, list, &self.plugin_manager)?
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
                play_on_layer::<true, _>(layer, &next, &self.plugin_manager)?;
                Ok(())
            },
            Some(Err(e)) => Err(e),
            Some(Ok(None)) | None => {
                layer.stop();
                Ok(())
            }
        }
    }

    /// Seeks in the audio source of the given layer according to
    /// [AudioSource::seek] with the given duration. Panics if the layer does
    /// not exist.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer on which to seek.
    /// * `delta`: The [SampleDuration] that determines the amount of time by
    ///   which is seeked. This is passed directly to [AudioSource::seek].
    ///
    /// # Errors
    ///
    /// Any [SeekOnLayerError] according to their respective documentations.
    pub fn seek_on_layer(&mut self, layer: &str, mut delta: SampleDuration)
            -> Result<(), SeekOnLayerError> {
        let layer = self.layers.get_mut(layer);

        if layer.active() {
            if layer.buffer.len() > 0 {
                if delta > SampleDuration::ZERO {
                    let advance = delta.samples()
                        .min(layer.buffer.len() as i64) as usize;
    
                    layer.buffer.advance_head(advance);
                    delta -= SampleDuration::from_samples(advance as i64);
                }
                else {
                    let retreat = (-delta.samples())
                        .min(layer.buffer.len() as i64) as usize;

                    layer.buffer.len -= retreat;
                }
            }

            if let Some(source) = layer.source.as_mut() {
                Ok(source.seek(delta)?)
            }
            else {
                Ok(())
            }
        }
        else {
            Err(SeekOnLayerError::LayerNotActive(layer.name().to_owned()))
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

    /// Gets [AudioMetadata] about the track currently played by the audio
    /// source on the layer with the given name.
    ///
    /// # Arguments
    ///
    /// * `layer`: The name of the layer on which to get the metadata.
    ///
    /// # Returns
    ///
    /// A new [AudioMetadata] instance containing information about the track
    /// played on the given layer.
    ///
    /// # Errors
    ///
    /// Any [LayerMetadataError] according to their respective documentation.
    pub fn layer_metadata(&self, layer: &str)
            -> Result<AudioMetadata, LayerMetadataError> {
        if !self.contains_layer(layer) {
            return
                Err(LayerMetadataError::LayerDoesNotExist(layer.to_owned()));
        }

        let layer = self.layers.get(layer);

        if let Some(source) = &layer.source {
            Ok(source.metadata())
        }
        else {
            Err(LayerMetadataError::LayerNotActive(layer.name.clone()))
        }
    }
}

impl AudioSource for Mixer {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let mut size = usize::MAX;
        let mut active_layers = Vec::new();

        for layer in self.layers.iter_mut() {
            if layer.active() && layer.buffer.len() < buf.len() {
                layer.read_from_source(buf.len(), &self.plugin_manager)?;
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

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("mixer has no child")
    }

    fn metadata(&self) -> AudioMetadata {
        panic!("mixer has no metadata")
    }
}

/// A wrapper of an [AudioSource] that implements the [Read] trait. It outputs
/// 32-bit floating-point PCM audio data.
pub struct PCMRead<S: AudioSource + Send> {
    source: Arc<RwLock<S>>,
    sample_buf: Vec<Sample>,

    #[cfg(feature = "bench")]
    bench_sample_count: usize,

    #[cfg(feature = "bench")]
    bench_duration: Duration
}

impl<S: AudioSource + Send> PCMRead<S> {

    /// Creates a new PCMRead with the given audio source.
    pub fn new(source: Arc<RwLock<S>>) -> PCMRead<S> {
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

const CHANNEL_SIZE: usize = 4;
const SAMPLE_SIZE: usize = 2 * CHANNEL_SIZE;

#[cfg(feature = "bench")]
const SAMPLES_FOR_REPORT: usize = 96000;

fn to_bytes(buf: &mut [u8], s: &Sample) {
    buf[..CHANNEL_SIZE].copy_from_slice(&s.left.to_le_bytes());
    buf[CHANNEL_SIZE..SAMPLE_SIZE].copy_from_slice(&s.right.to_le_bytes());
}

impl<S: AudioSource + Send> Read for PCMRead<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(feature = "bench")]
        let before = Instant::now();

        let sample_capacity = buf.len() / SAMPLE_SIZE;

        if self.sample_buf.len() < sample_capacity {
            self.sample_buf = vec![Sample::ZERO; sample_capacity];
        }

        let sample_len = self.source.write().unwrap()
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

impl<S: AudioSource + Send + Sync> MediaSource for PCMRead<S> {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use rambot_api::{
        AudioDocumentation,
        AudioMetadataBuilder,
        AudioSourceListResolver,
        AudioSourceResolver,
        PluginGuildConfig
    };

    use rambot_test_util::{MockAudioSource, MockAudioSourceList};

    use std::sync::Mutex;

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
        let mut pcm_read = PCMRead::new(Arc::new(RwLock::new(source)));
        let mut buf = vec![1; 1024];

        assert_eq!(800, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().enumerate()
            .all(|(i, b)| (i < 800 && b == 0) || (i >= 800 && b == 1)));
    }

    #[test]
    fn pcm_read_zeros_split() {
        let source = MockAudioSource::new(vec![Sample::ZERO; 100]);
        let mut pcm_read = PCMRead::new(Arc::new(RwLock::new(source)));
        let mut buf = vec![1; 256];

        assert_eq!(256, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().all(|b| b == 0));
        
        let mut buf = vec![1; 1024];

        assert_eq!(544, pcm_read_to_end(&mut buf, &mut pcm_read));
        assert!(buf.into_iter().enumerate()
            .all(|(i, b)| (i < 544 && b == 0) || (i >= 544 && b == 1)));
    }

    const TEST_1_LEN: usize = 48000;
    const TEST_2_LEN: usize = 48000;

    fn test_audio_1() -> Vec<Sample> {
        rambot_test_util::test_data(TEST_1_LEN, 60.0, 120.0)
    }

    fn test_audio_2() -> Vec<Sample> {
        rambot_test_util::test_data(TEST_1_LEN, 180.0, 240.0)
    }

    fn test_audio_sum() -> Vec<Sample> {
        rambot_test_util::sum_audio(&test_audio_1(), &test_audio_2())
    }

    fn set_audio<S>(mixer: &mut Mixer, layer: &str, audio_source: S)
    where
        S: AudioSource + Send + Sync + 'static
    {
        mixer.layers.get_mut(layer).source = Some(Box::new(audio_source));
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

        set_audio(mixer, name, audio);
    }

    fn mock_mixer() -> Mixer {
        Mixer::new(Arc::new(PluginManager::empty()))
    }

    #[test]
    fn mixer_single_audio_source() {
        let mut mixer = mock_mixer();
        add_layer(&mut mixer, "test", test_audio_1(), None);
        let result = rambot_test_util::read_to_end(&mut mixer).unwrap();

        rambot_test_util::assert_approximately_equal(test_audio_1(), result);
    }

    #[test]
    fn mixer_two_audio_sources() {
        let mut mixer = mock_mixer();
        add_layer(&mut mixer, "test1", test_audio_1(), None);
        add_layer(&mut mixer, "test2", test_audio_2(), None);
        let result = rambot_test_util::read_to_end(&mut mixer).unwrap();

        rambot_test_util::assert_approximately_equal(test_audio_sum(), result);
    }

    #[test]
    fn mixer_two_segmented_audio_sources() {
        let mut mixer = mock_mixer();
        add_layer(&mut mixer, "test1", test_audio_1(), Some(5));
        add_layer(&mut mixer, "test2", test_audio_2(), Some(7));
        let result =
            rambot_test_util::read_to_end_segmented(&mut mixer, 11).unwrap();

        rambot_test_util::assert_approximately_equal(test_audio_sum(), result);
    }

    struct MockAudioSourceResolver;

    impl AudioSourceResolver for MockAudioSourceResolver {
        fn documentation(&self) -> AudioDocumentation {
            panic!("mock audio source resolver asked for documentation")
        }

        fn can_resolve(&self, descriptor: &str, _: PluginGuildConfig) -> bool {
            descriptor == "1" || descriptor == "2"
        }

        fn resolve(&self, descriptor: &str, _: PluginGuildConfig)
                -> Result<Box<dyn AudioSource + Send + Sync>, String> {
            let samples = match descriptor {
                "1" => test_audio_1(),
                "2" => test_audio_2(),
                _ => panic!("invalid descriptor for mock audio source")
            };

            Ok(Box::new(
                MockAudioSource::with_normally_distributed_segment_size(
                    samples, 128.0, 32.0).unwrap()
            ))
        }
    }

    struct MockAudioSourceListResolver;

    impl AudioSourceListResolver for MockAudioSourceListResolver {
        fn documentation(&self) -> AudioDocumentation {
            panic!("mock audio source list resolver asked for documentation")
        }

        fn can_resolve(&self, descriptor: &str, _: PluginGuildConfig) -> bool {
            descriptor.split(',').count() > 1
        }

        fn resolve(&self, descriptor: &str, _: PluginGuildConfig)
                -> Result<Box<dyn AudioSourceList + Send + Sync>, String> {
            let entries = descriptor.split(',')
                .map(|s| s.to_owned())
                .collect::<Vec<_>>();

            Ok(Box::new(MockAudioSourceList::new(entries)))
        }
    }

    fn registered_mixer() -> Mixer {
        let mut plugin_manager = PluginManager::empty();
        let mut registry = plugin_manager.mock_registry();

        registry.register_audio_source_resolver(MockAudioSourceResolver);
        registry.register_audio_source_list_resolver(
            MockAudioSourceListResolver);
        drop(registry);

        Mixer::new(Arc::new(plugin_manager))
    }

    fn play(mixer: &mut Mixer, layer: &str, descriptor: &str)
            -> Result<(), io::Error> {
        mixer.play_on_layer(
            layer, descriptor, Default::default(), no_callback())
    }

    const RANDOM_TEST_ITERATORS: usize = 64;

    #[test]
    #[should_panic]
    fn play_on_nonexistent_layer() {
        let mut mixer = registered_mixer();
        let _ = play(&mut mixer, "l", "1");
    }

    #[test]
    fn play_unresolveable_audio_source() {
        let mut mixer = registered_mixer();
        mixer.add_layer("l");
        let res = play(&mut mixer, "l", "#");

        assert!(res.is_err());
        assert!(!mixer.active());
    }

    #[test]
    fn play_single_audio_source() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("l");
            play(&mut mixer, "l", "1").unwrap();
    
            assert!(mixer.active());
    
            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();
    
            rambot_test_util::assert_approximately_equal(
                test_audio_1(), audio);
            assert!(!mixer.active());
        }
    }

    #[test]
    fn play_playlist() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("l");
            play(&mut mixer, "l", "1,2,1").unwrap();

            assert!(mixer.active());

            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();
            let mut expected = test_audio_1();
            expected.append(&mut test_audio_2());
            expected.append(&mut test_audio_1());

            rambot_test_util::assert_approximately_equal(expected, audio);
            assert!(!mixer.active());
        }
    }

    #[test]
    fn skip_during_single_audio_source() {
        let mut mixer = registered_mixer();
        mixer.add_layer("l");
        play(&mut mixer, "l", "1").unwrap();

        assert!(mixer.read(&mut [Sample::ZERO; 10]).unwrap() > 0);

        mixer.skip_on_layer("l").unwrap();

        assert!(!mixer.active());
        assert_eq!(0, mixer.read(&mut [Sample::ZERO; 10]).unwrap());
    }

    #[test]
    fn skip_during_playlist() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("l");
            play(&mut mixer, "l", "1,2,1").unwrap();

            assert!(mixer.read(&mut [Sample::ZERO; 10]).unwrap() > 0);

            mixer.skip_on_layer("l").unwrap();

            assert!(mixer.active());

            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();
            let mut expected = test_audio_2();
            expected.append(&mut test_audio_1());

            rambot_test_util::assert_approximately_equal(expected, audio);
            assert!(!mixer.active());
        }
    }

    #[test]
    fn skip_end_of_playlist() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("l");
            play(&mut mixer, "l", "1,2,1").unwrap();

            let mut total = 0;

            while total <= TEST_1_LEN + TEST_2_LEN {
                let count = mixer.read(&mut [Sample::ZERO; 10]).unwrap();
                total += count;

                assert!(count > 0);
            }

            mixer.skip_on_layer("l").unwrap();

            assert!(!mixer.active());
            assert_eq!(0, mixer.read(&mut [Sample::ZERO; 10]).unwrap());
        }
    }

    #[test]
    fn stop_layer() {
        let mut mixer = registered_mixer();
        mixer.add_layer("l");
        play(&mut mixer, "l", "1,2,1").unwrap();
        mixer.stop_layer("l");

        assert!(!mixer.active());
    }

    #[test]
    fn stop_all() {
        let mut mixer = registered_mixer();
        mixer.add_layer("l");
        play(&mut mixer, "l", "1,2,1").unwrap();
        mixer.stop_all();

        assert!(!mixer.active());
    }

    #[test]
    fn mid_playlist_resolution_fail() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let error = Arc::new(Mutex::new(false));
            let error_clone = Arc::clone(&error);
            let mut mixer = registered_mixer();
            mixer.add_layer("l");
            mixer.play_on_layer("l", "1,#,1", Default::default(), move |_, _| {
                *error_clone.lock().unwrap() = true;
            }).unwrap();

            assert!(mixer.active());

            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();

            rambot_test_util::assert_approximately_equal(test_audio_1(), audio);
            assert!(!mixer.active());
            assert!(*error.lock().unwrap());
        }
    }

    #[test]
    fn two_overlayed_playlists() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("a");
            mixer.add_layer("b");
            play(&mut mixer, "a", "1,2").unwrap();
            play(&mut mixer, "b", "2,2").unwrap();

            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();
            let mut expected = test_audio_sum();

            for sample in test_audio_2() {
                expected.push(sample + sample);
            }

            assert!(!mixer.active());
            rambot_test_util::assert_approximately_equal(expected, audio);
        }
    }

    #[test]
    fn non_failed_layers_continue_on_error() {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = registered_mixer();
            mixer.add_layer("a");
            mixer.add_layer("b");
            play(&mut mixer, "a", "1,2").unwrap();
            play(&mut mixer, "b", "2,#").unwrap();

            assert!(mixer.active());

            let audio = rambot_test_util::read_to_end(&mut mixer).unwrap();
            let mut expected = test_audio_sum();
            expected.append(&mut test_audio_2());

            assert!(!mixer.active());
            rambot_test_util::assert_approximately_equal(expected, audio);
        }
    }

    fn test_metadata_1() -> AudioMetadata {
        AudioMetadataBuilder::new()
            .with_title("test title")
            .with_album("test album")
            .build()
    }

    fn test_metadata_2() -> AudioMetadata {
        AudioMetadataBuilder::new()
            .with_artist("test artist")
            .with_year(1337)
            .build()
    }

    #[test]
    fn two_layer_metadata_query() {
        let mut mixer = mock_mixer();
        mixer.add_layer("test_1");
        mixer.add_layer("test_2");

        let test_source_1 =
            MockAudioSource::with_metadata(test_audio_1(), test_metadata_1());
        let test_source_2 =
            MockAudioSource::with_metadata(test_audio_2(), test_metadata_2());

        set_audio(&mut mixer, "test_1", test_source_1);
        set_audio(&mut mixer, "test_2", test_source_2);

        assert_eq!(test_metadata_1(), mixer.layer_metadata("test_1").unwrap());
        assert_eq!(test_metadata_2(), mixer.layer_metadata("test_2").unwrap());
    }

    #[test]
    fn metadata_query_on_non_existent_layer() {
        let mut mixer = mock_mixer();
        mixer.add_layer("test_1");

        let test_source_1 =
            MockAudioSource::with_metadata(test_audio_1(), test_metadata_1());

        set_audio(&mut mixer, "test_1", test_source_1);

        assert_eq!(LayerMetadataError::LayerDoesNotExist("test_2".to_owned()),
            mixer.layer_metadata("test_2").unwrap_err());
    }

    #[test]
    fn metadata_query_on_inactive_layer() {
        let mut mixer = mock_mixer();
        mixer.add_layer("test_1");
        mixer.add_layer("test_2");

        let test_source_1 =
            MockAudioSource::with_metadata(test_audio_1(), test_metadata_1());

        set_audio(&mut mixer, "test_1", test_source_1);

        assert_eq!(LayerMetadataError::LayerNotActive("test_2".to_owned()),
            mixer.layer_metadata("test_2").unwrap_err());
    }

    fn test_seek(samples: i64) {
        for _ in 0..RANDOM_TEST_ITERATORS {
            let mut mixer = mock_mixer();
            add_layer(&mut mixer, "test1", test_audio_1(), None);
            let mut buf = [Sample::ZERO; TEST_1_LEN / 3];
            let count = mixer.read(&mut buf).unwrap();
            mixer.seek_on_layer("test1", SampleDuration::from_samples(samples))
                .unwrap();
            let result = rambot_test_util::read_to_end(&mut mixer).unwrap();

            rambot_test_util::assert_approximately_equal(
                &test_audio_1()[..count], &buf[..count]);
            rambot_test_util::assert_approximately_equal(
                &test_audio_1()[((count as i64 + samples) as usize)..], &result)
        }
    }

    #[test]
    fn seek_forward_short() {
        test_seek(10);
    }

    #[test]
    fn seek_forward_long() {
        test_seek((TEST_1_LEN / 3) as i64);
    }
}
