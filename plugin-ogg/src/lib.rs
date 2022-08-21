use id3::Timestamp;

use lewton::inside_ogg::OggStreamReader;

use plugin_commons::{FileManager, OpenedFile, SeekWrapper};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioMetadata,
    AudioMetadataBuilder,
    AudioSource,
    AudioSourceResolver,
    ResolverRegistry,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    Sample
};

use std::io::{self, ErrorKind, Read, Seek};

struct OggAudioSource<R: Read + Seek> {
    reader: OggStreamReader<R>,
    remaining: Vec<Sample>,
    remaining_idx: usize,
    fallback_title: String
}

impl<R: Read + Seek> OggAudioSource<R> {
    fn extend_remaining(&mut self, packet: Vec<Vec<f32>>) {
        self.remaining.clear();
        self.remaining_idx = 0;

        if packet.len() == 1 {
            // Mono

            self.remaining.extend(packet[0].iter()
                .map(|&value| Sample::mono(value)));
        }
        else {
            // Stereo or more

            self.remaining.extend(packet[0].iter()
                .zip(packet[1].iter())
                .map(|(&left, &right)| Sample {
                    left,
                    right
                }))
        }
    }
}

impl<R: Read + Seek> AudioSource for OggAudioSource<R> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        loop {
            let remaining_len = self.remaining.len() - self.remaining_idx;

            if remaining_len > buf.len() {
                let start = self.remaining_idx;
                let end = start + buf.len();
                buf.copy_from_slice(&self.remaining[start..end]);
                self.remaining_idx = end;
                return Ok(buf.len());
            }
            else if remaining_len > 0 {
                let start = self.remaining_idx;
                buf[..remaining_len].copy_from_slice(&self.remaining[start..]);
                self.remaining_idx = self.remaining.len();
                return Ok(remaining_len);
            }
            else {
                let packet = self.reader.read_dec_packet_generic::<Vec<Vec<f32>>>()
                    .map_err(|e|
                        io::Error::new(ErrorKind::Other, format!("{}", e)))?;

                match packet {
                    Some(packet) => self.extend_remaining(packet),
                    None => return Ok(0)
                }
            }
        }
    }

    fn has_child(&self) -> bool {
        false
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        panic!("ogg audio source has no child")
    }

    fn metadata(&self) -> AudioMetadata {
        let mut meta_builder = AudioMetadataBuilder::new()
            .with_title(&self.fallback_title);

        for (key, value) in self.reader.comment_hdr.comment_list.iter() {
            match key.as_str() {
                "TITLE" => meta_builder = meta_builder.with_title(value),
                "ARTIST" => meta_builder = meta_builder.with_artist(value),
                "ALBUM" => meta_builder = meta_builder.with_album(value),
                "DATE" => {
                    if let Ok(date) = value.parse::<Timestamp>() {
                        meta_builder = meta_builder.with_year(date.year);
                    }
                },
                _ => { }
            }
        }

        meta_builder.build()
    }
}

struct OggAudioSourceResolver {
    file_manager: FileManager
}

impl OggAudioSourceResolver {
    fn resolve_reader<R>(&self, reader: R, descriptor: &str)
        -> Result<Box<dyn AudioSource + Send + Sync>, String>
    where
        R: Read + Seek + Send + Sync + 'static
    {
        let ogg_reader = OggStreamReader::new(reader)
            .map_err(|e| format!("{}", e))?;
        let sampling_rate = ogg_reader.ident_hdr.audio_sample_rate;

        Ok(plugin_commons::adapt_sampling_rate(OggAudioSource {
            reader: ogg_reader,
            remaining: Vec::new(),
            remaining_idx: 0,
            fallback_title: descriptor.to_owned()
        }, sampling_rate))
    }
}

impl AudioSourceResolver for OggAudioSourceResolver {

    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to an `.ogg` file on the internet can be \
                provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Ogg")
            .with_summary("Playback OGG audio files.")
            .with_description(format!("Specify the path of a file with the \
                `.ogg` extension relative to the bot root directory. {}This \
                plugin will playback the given file.", web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> bool {
        self.file_manager.is_file_with_extension(
            descriptor, &guild_config, ".ogg")
    }

    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send + Sync>, String> {
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;
    
        match file {
            OpenedFile::Local(reader) =>
                self.resolve_reader(reader, descriptor),
            OpenedFile::Web(reader) =>
                self.resolve_reader(SeekWrapper::new(reader), descriptor)
        }
    }
}

struct OggPlugin;

impl Plugin for OggPlugin {

    fn load_plugin<'registry>(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_audio_source_resolver(OggAudioSourceResolver {
            file_manager: FileManager::new(config)
        });

        Ok(())
    }
}

fn make_ogg_plugin() -> OggPlugin {
    OggPlugin
}

rambot_api::export_plugin!(make_ogg_plugin);
