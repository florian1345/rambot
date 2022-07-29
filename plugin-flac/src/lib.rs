// TODO occasionally check for ouroboros update that mitigates this issue
#![allow(clippy::drop_non_drop)]

use claxon::FlacReader;
use claxon::frame::{Block, FrameReader};
use claxon::input::BufferedReader;

use plugin_commons::{FileManager, OpenedFile};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSource,
    AudioSourceResolver,
    Plugin,
    PluginConfig,
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
    factor: f32
}

impl<R: Read + 'static> FlacAudioSource<R> {
    fn new(reader: FlacReader<R>) -> FlacAudioSource<R> {
        let bits_per_sample = reader.streaminfo().bits_per_sample;
        let factor = 1.0 / (1 << (bits_per_sample - 1)) as f32;

        FlacAudioSource {
            frames_self_ref: FramesSelfRefBuilder {
                reader: Box::new(reader),
                frames_builder: |reader| reader.blocks()
            }.build(),
            block: Block::empty(),
            offset: 0,
            factor
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
                let value = self.sample_f32(0, i + self.offset);

                *sample = Sample {
                    left: value,
                    right: value
                }
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

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("flac audio source has no child")
    }
}

struct FlacAudioSourceResolver {
    file_manager: FileManager
}

impl FlacAudioSourceResolver {
    fn resolve_reader<R>(&self, reader: R)
        -> Result<Box<dyn AudioSource + Send>, String>
    where
        R: Read + Send + 'static
    {
        let reader = FlacReader::new(reader)
            .map_err(|e| format!("{}", e))?;
        let sampling_rate = reader.streaminfo().sample_rate;

        Ok(plugin_commons::adapt_sampling_rate(
            FlacAudioSource::new(reader), sampling_rate))
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

    fn can_resolve(&self, descriptor: &str) -> bool {
        self.file_manager.is_file_with_extension(descriptor, ".flac")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let file = self.file_manager.open_file_buf(descriptor)?;

        match file {
            OpenedFile::Local(reader) => self.resolve_reader(reader),
            OpenedFile::Web(reader) => self.resolve_reader(reader)
        }
    }
}

struct FlacPlugin;

impl Plugin for FlacPlugin {

    fn load_plugin<'registry>(&mut self, config: &PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
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
