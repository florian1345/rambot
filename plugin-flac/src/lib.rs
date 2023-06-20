use claxon::FlacReader;
use claxon::frame::{Block, FrameReader};
use claxon::input::BufferedReader;

use id3::Timestamp;

use plugin_commons::{FileManager, OpenedFile};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioMetadata,
    AudioMetadataBuilder,
    AudioSource,
    AudioSourceResolver,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolverRegistry,
    Sample
};

use std::io::{self, ErrorKind, Read};
use std::mem;

#[ouroboros::self_referencing]
struct FramesSelfRef<R: Read + 'static> {
    reader: Box<FlacReader<R>>,
    #[borrows(mut reader)]
    #[covariant]
    frames: FrameReader<&'this mut BufferedReader<R>>
}

struct FlacAudioSource<R: Read + 'static> {
    frames_self_ref: FramesSelfRef<R>,
    block: Block,
    offset: usize,
    factor: f32,
    metadata: AudioMetadata
}

impl<R: Read + 'static> FlacAudioSource<R> {
    fn new(reader: FlacReader<R>, metadata: AudioMetadata)
            -> FlacAudioSource<R> {
        let bits_per_sample = reader.streaminfo().bits_per_sample;
        let factor = 1.0 / (1 << (bits_per_sample - 1)) as f32;

        FlacAudioSource {
            frames_self_ref: FramesSelfRefBuilder {
                reader: Box::new(reader),
                frames_builder: |reader| reader.blocks()
            }.build(),
            block: Block::empty(),
            offset: 0,
            factor,
            metadata
        }
    }

    fn sample_f32(&self, ch: u32, sample: usize) -> f32 {
        self.factor * self.block.sample(ch, sample as u32) as f32
    }
}

impl<R: Read> AudioSource for FlacAudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        if self.offset >= self.block.duration() as usize {
            self.offset = 0;
            let block = mem::replace(&mut self.block, Block::empty());

            let next_frame_res = self.frames_self_ref
                .with_frames_mut(|f| f.read_next_or_eof(block.into_buffer()));

            match next_frame_res {
                Ok(Some(block)) => self.block = block,
                Ok(None) => return Ok(0),
                Err(e) => return
                    Err(io::Error::new(ErrorKind::Other, e.to_string()))
            }
        }

        if self.block.channels() == 0 {
            return Ok(0);
        }

        let len =
            (self.block.duration() as usize - self.offset).min(buf.len());

        if self.block.channels() == 1 {
            for (i, sample) in buf[..len].iter_mut().enumerate() {
                *sample = Sample::mono(self.sample_f32(0, i + self.offset));
            }
        }
        else {
            for (i, sample) in buf[..len].iter_mut().enumerate() {
                *sample = Sample {
                    left: self.sample_f32(0, i + self.offset),
                    right: self.sample_f32(1, i + self.offset)
                }
            }
        }

        self.offset += len;
        Ok(len)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("flac audio source has no child")
    }

    fn metadata(&self) -> AudioMetadata {
        self.metadata.clone()
    }
}

fn read_tag<'a, R, S>(reader: &FlacReader<R>, tag_name: &str, set: S)
where
    R: Read + Send + Sync + 'static,
    S: FnOnce(&str) -> &'a mut AudioMetadataBuilder
{
    if let Some(value) = reader.get_tag(tag_name).next() {
        set(value);
    }
}

struct FlacAudioSourceResolver {
    file_manager: FileManager
}

impl FlacAudioSourceResolver {
    fn resolve_reader<R>(&self, reader: R, descriptor: &str)
        -> Result<Box<dyn AudioSource + Send + Sync>, String>
    where
        R: Read + Send + Sync + 'static
    {
        let reader = FlacReader::new(reader)
            .map_err(|e| format!("{}", e))?;
        let mut meta_builder = AudioMetadataBuilder::new();

        if let Some(title) = reader.get_tag("TITLE").next() {
            meta_builder = meta_builder.with_title(title);
        }
        else {
            meta_builder = meta_builder.with_title(descriptor);
        }

        read_tag(&reader, "WORK", |a| meta_builder.set_super_title(a));
        read_tag(&reader, "ARTIST", |a| meta_builder.set_artist(a));
        read_tag(&reader, "COMPOSER", |a| meta_builder.set_composer(a));
        read_tag(&reader, "CONDUCTOR", |a| meta_builder.set_conductor(a));
        read_tag(&reader, "ORGANISATION", |a| meta_builder.set_publisher(a));
        read_tag(&reader, "ALBUM", |a| meta_builder.set_album(a));
        read_tag(&reader, "GENRE", |a| meta_builder.set_genre(a));

        if let Some(track) = reader.get_tag("TRACKNUMBER").next() {
            if let Ok(track) = track.parse() {
                meta_builder.set_track(track);
            }
        }

        if let Some(date) = reader.get_tag("DATE").next() {
            if let Ok(date) = date.parse::<Timestamp>() {
                meta_builder.set_year(date.year);
            }
        }

        let metadata = meta_builder.build();
        let sampling_rate = reader.streaminfo().sample_rate;

        Ok(plugin_commons::adapt_sampling_rate(
            FlacAudioSource::new(reader, metadata), sampling_rate))
    }
}

impl AudioSourceResolver for FlacAudioSourceResolver {

    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to a `.flac` file on the internet can be \
                provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Flac")
            .with_summary("Playback FLAC audio files.")
            .with_description(format!("Specify the path of a file with the \
                `.flac` extension relative to the bot root directory. {}This \
                plugin will playback the given file.", web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> bool {
        self.file_manager.is_file_with_extension(
            descriptor, &guild_config, ".flac")
    }

    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, String> {
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;

        match file {
            OpenedFile::Local(reader) =>
                self.resolve_reader(reader, descriptor),
            OpenedFile::Web(reader) => self.resolve_reader(reader, descriptor)
        }
    }
}

struct FlacPlugin;

impl Plugin for FlacPlugin {

    fn load_plugin(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'_>) -> Result<(), String> {
        registry.register_audio_source_resolver(FlacAudioSourceResolver {
            file_manager: FileManager::new(config)
        });
        Ok(())
    }
}

fn make_flac_plugin() -> FlacPlugin {
    FlacPlugin
}

rambot_api::export_plugin!(make_flac_plugin);
