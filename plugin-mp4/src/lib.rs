use plugin_commons::{FileManager, OpenedFile};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSource,
    AudioSourceResolver,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolverRegistry,
    Sample
};

use std::io::{self, ErrorKind, Read, Seek, SeekFrom};

use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Channels, Signal};
use symphonia::core::codecs::{self, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatReader, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::sample::{i24, u24};
use symphonia::core::sample::Sample as SymphoniaSample;
use symphonia::default::codecs::{
    AacDecoder,
    AlacDecoder,
    FlacDecoder,
    Mp3Decoder,
    VorbisDecoder
};
use symphonia::default::formats::IsoMp4Reader;

struct Mp4TrackDecoder<D> {
    reader: IsoMp4Reader,
    decoder: D,
    track_id: u32
}

impl<D: Decoder> Mp4TrackDecoder<D> {
    fn next_packet(&mut self) -> Result<bool, SymphoniaError> {
        loop {
            match self.reader.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != self.track_id {
                        continue;
                    }

                    match self.decoder.decode(&packet) {
                        Ok(_) => return Ok(true),
                        Err(e) => return Err(e)
                    }
                },
                Err(SymphoniaError::IoError(e))
                        if e.kind() == ErrorKind::UnexpectedEof => {
                    return Ok(false);
                },
                Err(e) => return Err(e)
            }
        }
    }
}

fn to_io_err(e: SymphoniaError) -> io::Error {
    match e {
        SymphoniaError::IoError(e) => e,
        e => io::Error::new(ErrorKind::Other, format!("{}", e))
    }
}

const HALF_U8_MAX_F32: f32 = u8::MAX as f32 * 0.5;
const HALF_U16_MAX_F32: f32 = u16::MAX as f32 * 0.5;
const HALF_U24_MAX_F32: f32 = u24::MAX.0 as f32 * 0.5;
const HALF_U32_MAX_F32: f32 = u32::MAX as f32 * 0.5;

const I8_ABS_MAX_F32: f32 = i8::MIN as f32 * (-0.5);
const I16_ABS_MAX_F32: f32 = i16::MIN as f32 * (-0.5);
const I24_ABS_MAX_F32: f32 = i24::MIN.0 as f32 * (-0.5);
const I32_ABS_MAX_F32: f32 = i32::MIN as f32 * (-0.5);

trait IntoF32 : Copy {
    fn into_f32(self) -> f32;
}

impl IntoF32 for u8 {
    fn into_f32(self) -> f32 {
        (self as f32 - HALF_U8_MAX_F32) / HALF_U8_MAX_F32
    }
}

impl IntoF32 for i8 {
    fn into_f32(self) -> f32 {
        self as f32 / I8_ABS_MAX_F32
    }
}

impl IntoF32 for u16 {
    fn into_f32(self) -> f32 {
        (self as f32 - HALF_U16_MAX_F32) / HALF_U16_MAX_F32
    }
}

impl IntoF32 for i16 {
    fn into_f32(self) -> f32 {
        self as f32 / I16_ABS_MAX_F32
    }
}

impl IntoF32 for u24 {
    fn into_f32(self) -> f32 {
        (self.0 as f32 - HALF_U24_MAX_F32) / HALF_U24_MAX_F32
    }
}

impl IntoF32 for i24 {
    fn into_f32(self) -> f32 {
        self.0 as f32 / I24_ABS_MAX_F32
    }
}

impl IntoF32 for u32 {
    fn into_f32(self) -> f32 {
        (self as f32 - HALF_U32_MAX_F32) / HALF_U32_MAX_F32
    }
}

impl IntoF32 for i32 {
    fn into_f32(self) -> f32 {
        self as f32 / I32_ABS_MAX_F32
    }
}

impl IntoF32 for f32 {
    fn into_f32(self) -> f32 {
        self
    }
}

impl IntoF32 for f64 {
    fn into_f32(self) -> f32 {
        self as f32
    }
}

struct Mp4AudioSource<D> {
    decoder: Mp4TrackDecoder<D>,
    left_channel_id: usize,
    right_channel_id: usize,
    frame_idx: usize
}

impl<D> Mp4AudioSource<D> {
    fn fill<S>(&self, buf: &mut [Sample], audio: &AudioBuffer<S>) -> usize
    where
        S: IntoF32 + SymphoniaSample
    {
        let left = &audio.chan(self.left_channel_id)[self.frame_idx..];
        let right = &audio.chan(self.right_channel_id)[self.frame_idx..];
        let len = left.len().min(right.len()).min(buf.len());

        for (i, sample) in buf[..len].iter_mut().enumerate() {
            *sample = Sample {
                left: left[i].into_f32(),
                right: right[i].into_f32()
            };
        }

        len
    }
}

impl<D: Decoder> AudioSource for Mp4AudioSource<D> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let last_decoded_frames = self.decoder.decoder.last_decoded().frames();
        let audio_available = if self.frame_idx >= last_decoded_frames {
            self.frame_idx = 0;
            self.decoder.next_packet().map_err(to_io_err)?
        }
        else {
            true
        };

        if !audio_available {
            return Ok(0);
        }

        let audio = self.decoder.decoder.last_decoded();
        let count = match audio {
            AudioBufferRef::U8(a) => self.fill(buf, &*a),
            AudioBufferRef::U16(a) => self.fill(buf, &*a),
            AudioBufferRef::U24(a) => self.fill(buf, &*a),
            AudioBufferRef::U32(a) => self.fill(buf, &*a),
            AudioBufferRef::S8(a) => self.fill(buf, &*a),
            AudioBufferRef::S16(a) => self.fill(buf, &*a),
            AudioBufferRef::S24(a) => self.fill(buf, &*a),
            AudioBufferRef::S32(a) => self.fill(buf, &*a),
            AudioBufferRef::F32(a) => self.fill(buf, &*a),
            AudioBufferRef::F64(a) => self.fill(buf, &*a)
        };

        self.frame_idx += count;

        Ok(count)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("mp4 audio source has no child")
    }
}

struct Unseekable<R> {
    read: R
}

impl<R: Read> Read for Unseekable<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read.read(buf)
    }
}

impl<R: Read> Seek for Unseekable<R> {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        panic!("cannot seek unseekable")
    }
}

impl<R: Read + Send + Sync> MediaSource for Unseekable<R> {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
enum ChannelQuality {
    NoChannels,
    NotStereo,
    ContainsStereo,
    IsStereo
}

fn get_channel_quality(channels: Channels) -> ChannelQuality {
    let stereo = Channels::FRONT_LEFT | Channels::FRONT_RIGHT;

    if channels == stereo {
        ChannelQuality::IsStereo
    }
    else if channels.contains(stereo) {
        ChannelQuality::ContainsStereo
    }
    else {
        ChannelQuality::NotStereo
    }
}

fn get_track_channel_quality(track: &Track) -> ChannelQuality {
    if let Some(channels) = track.codec_params.channels {
        get_channel_quality(channels)
    }
    else {
        ChannelQuality::NoChannels
    }
}

fn select_best_track(reader: &IsoMp4Reader) -> Option<&Track> {
    if let Some(track) = reader.default_track() {
        Some(track)
    }
    else {
        reader.tracks().iter().max_by_key(|t| get_track_channel_quality(*t))
    }
}

fn select_channels(channels: Channels) -> (usize, usize) {
    // TODO consider weirder forms of stereo

    if get_channel_quality(channels) >= ChannelQuality::ContainsStereo {
        (0, 1)
    }
    else {
        (0, 0)
    }
}

fn construct_source<D>(reader: IsoMp4Reader, track: &Track)
    -> Result<Box<dyn AudioSource + Send + Sync>, String>
where
    D: Decoder + 'static
{
    let decoder = D::try_new(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("{}", e))?;
    let mut mp4_decoder = Mp4TrackDecoder {
        reader,
        decoder,
        track_id: track.id
    };
    let has_packet = mp4_decoder.next_packet().map_err(|e| format!("{}", e))?;

    if !has_packet {
        return Err("Empty audio file.".to_owned());
    }

    let first_packet = mp4_decoder.decoder.last_decoded();
    let spec = first_packet.spec();
    let (left_channel_id, right_channel_id) = select_channels(spec.channels);
    let sampling_rate = spec.rate;

    Ok(Box::new(plugin_commons::adapt_sampling_rate(Mp4AudioSource {
        decoder: mp4_decoder,
        left_channel_id,
        right_channel_id,
        frame_idx: 0
    }, sampling_rate)))
}

fn resolve_reader<R>(reader: R) -> Result<Box<dyn AudioSource + Send + Sync>, String>
where
    R: Read + Send + Sync + 'static
{
    let media_source = Unseekable {
        read: reader
    };
    let media_source_stream =
        MediaSourceStream::new(Box::new(media_source), Default::default());
    let reader =
        IsoMp4Reader::try_new(media_source_stream, &Default::default())
            .map_err(|e| format!("{}", e))?;
    let track = select_best_track(&reader);

    if track.is_none() {
        return Err("No viable audio track found.".to_owned());
    }

    let track = track.unwrap().clone();

    match track.codec_params.codec {
        codecs::CODEC_TYPE_AAC =>
            construct_source::<AacDecoder>(reader, &track),
        codecs::CODEC_TYPE_ALAC =>
            construct_source::<AlacDecoder>(reader, &track),
        codecs::CODEC_TYPE_FLAC =>
            construct_source::<FlacDecoder>(reader, &track),
        codecs::CODEC_TYPE_MP3 =>
            construct_source::<Mp3Decoder>(reader, &track),
        codecs::CODEC_TYPE_VORBIS =>
            construct_source::<VorbisDecoder>(reader, &track),
        _ => Err("Unsupported codec.".to_owned())
    }
}

struct Mp4AudioSourceResolver {
    file_manager: FileManager
}

impl AudioSourceResolver for Mp4AudioSourceResolver {
    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to an MPEG-4 audio file on the internet can \
                be provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Mp4")
            .with_summary("Playback MPEG-4 audio files.")
            .with_description(format!("Specify the path of an MPEG-4 audio \
                file (identified by the `.mp4`, `.m4a`, or `.m4b` extension) \
                relative to the bot root directory. {}This plugin will \
                playback the given file.", web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> bool {
        for ext in [ ".mp4", ".m4a", ".m4b" ] {
            let is_file_with_ext = self.file_manager.is_file_with_extension(
                descriptor, &guild_config, ext);

            if is_file_with_ext {
                return true;
            }
        }

        false
    }

    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, String> {
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;

        match file {
            OpenedFile::Local(reader) => resolve_reader(reader),
            OpenedFile::Web(reader) => resolve_reader(reader)
        }
    }
}

struct Mp4Plugin;

impl Plugin for Mp4Plugin {
    fn load_plugin<'registry>(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_audio_source_resolver(Mp4AudioSourceResolver {
            file_manager: FileManager::new(config)
        });

        Ok(())
    }
}

fn make_mp4_plugin() -> Mp4Plugin {
    Mp4Plugin
}

rambot_api::export_plugin!(make_mp4_plugin);
