use minimp3::{self, Decoder, Frame};

use plugin_commons::{FileManager, OpenedFile};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioMetadata,
    AudioSource,
    AudioSourceResolver,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolverRegistry,
    Sample
};

use std::io::{self, ErrorKind, Read};

struct FrameIterator<R> {
    decoder: Decoder<R>
}

impl<R: Read> Iterator for FrameIterator<R> {
    type Item = Result<Frame, minimp3::Error>;

    fn next(&mut self) -> Option<Result<Frame, minimp3::Error>> {
        match self.decoder.next_frame() {
            Ok(frame) => Some(Ok(frame)),
            Err(minimp3::Error::Eof) => None,
            Err(e) => Some(Err(e))
        }
    }
}

fn to_f32(i: i16) -> f32 {
    const FACTOR: f32 = 1.0 / 32768.0;

    i as f32 * FACTOR
}

struct Mp3AudioSource<R: Read> {
    frames: FrameIterator<R>,
    current_frame: Frame,
    current_frame_idx: usize,
    metadata: AudioMetadata
}

impl<R: Read> AudioSource for Mp3AudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        if self.current_frame_idx >= self.current_frame.data.len() {
            if let Some(next_frame) = self.frames.next() {
                self.current_frame = next_frame
                    .map_err(|e|
                        io::Error::new(ErrorKind::Other, format!("{}", e)))?;
                self.current_frame_idx = 0;
            }
            else {
                return Ok(0);
            }
        }

        let channels = self.current_frame.channels;
        let remaining_data =
            &self.current_frame.data[self.current_frame_idx..];
        let sample_count = (remaining_data.len() / channels).min(buf.len());

        if channels == 1 {
            for sample_idx in 0..sample_count {
                buf[sample_idx] =
                    Sample::mono(to_f32(remaining_data[sample_idx]));
            }
        }
        else {
            for (sample_idx, sample) in buf.iter_mut().enumerate().take(sample_count) {
                let data_idx = sample_idx * channels;
                let left = to_f32(remaining_data[data_idx]);
                let right = to_f32(remaining_data[data_idx + 1]);

                *sample = Sample {
                    left,
                    right
                };
            }
        }

        self.current_frame_idx += sample_count * channels;
        Ok(sample_count)
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("mp3 audio source has no child")
    }

    fn metadata(&self) -> AudioMetadata {
        self.metadata.clone()
    }
}

struct Mp3AudioSourceResolver {
    file_manager: FileManager
}

fn resolve_mp3_reader<R>(reader: R, metadata: AudioMetadata)
    -> Result<Box<dyn AudioSource + Send + Sync>, String>
where
    R: Read + Send + Sync + 'static
{
    let decoder = Decoder::new(reader);
    let mut frames = FrameIterator {
        decoder
    };
    let first_frame = frames.next()
        .ok_or_else(|| "File is empty.".to_owned())?
        .map_err(|e| format!("{}", e))?;
    let sampling_rate = first_frame.sample_rate as u32;
    Ok(plugin_commons::adapt_sampling_rate(Mp3AudioSource {
        frames,
        current_frame: first_frame,
        current_frame_idx: 0,
        metadata
    }, sampling_rate))
}

impl AudioSourceResolver for Mp3AudioSourceResolver {

    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to an `.mp3` file on the internet can be \
                provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Mp3")
            .with_summary("Playback MP3 audio files.")
            .with_description(format!("Specify the path of a file with the \
                `.mp3` extension relative to the bot root directory. {}This \
                plugin will playback the given file.", web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> bool {
        self.file_manager.is_file_with_extension(
            descriptor, &guild_config, ".mp3")
    }

    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, String> {
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;
        let metadata = plugin_commons::metadata_from_file(file, descriptor)?;
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;

        match file {
            OpenedFile::Local(reader) => resolve_mp3_reader(reader, metadata),
            OpenedFile::Web(reader) => resolve_mp3_reader(reader, metadata)
        }
    }
}

struct Mp3Plugin;

impl Plugin for Mp3Plugin {

    fn load_plugin(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'_>) -> Result<(), String> {
        registry.register_audio_source_resolver(Mp3AudioSourceResolver {
            file_manager: FileManager::new(config)
        });

        Ok(())
    }
}

fn make_mp3_plugin() -> Mp3Plugin {
    Mp3Plugin
}

rambot_api::export_plugin!(make_mp3_plugin);
