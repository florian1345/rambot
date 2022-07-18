use lewton::inside_ogg::OggStreamReader;

use rambot_api::{
    AdapterResolver,
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    Sample
};

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Seek};

fn fill<T, I>(slice: &mut [T], iter: I)
where
    I: Iterator<Item = T>
{
    for (i, t) in iter.enumerate() {
        slice[i] = t;
    }
}

struct OggAudioSource<R: Read + Seek> {
    reader: OggStreamReader<R>,
    remaining: VecDeque<Sample>
}

impl<R: Read + Seek> OggAudioSource<R> {
    fn extend_remaining(&mut self, packet: Vec<Vec<f32>>) {
        if packet.len() == 1 {
            // Mono

            self.remaining.extend(packet[0].iter()
                .map(|&sample| Sample {
                    left: sample,
                    right: sample
                }));
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
            let remaining_len = self.remaining.len();

            if remaining_len > buf.len() {
                fill(buf, self.remaining.drain(..buf.len()));
                return Ok(buf.len());
            }
            else if remaining_len > 0 {
                fill(buf, self.remaining.drain(..));
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

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        panic!("ogg audio source has no child")
    }
}

struct OggAudioSourceResolver;

impl AudioSourceResolver for OggAudioSourceResolver {

    fn can_resolve(&self, descriptor: &str) -> bool {
        plugin_commons::is_file_with_extension(descriptor, ".ogg")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, String> {
        let reader = plugin_commons::open_file_buf(descriptor)?;
        let ogg_reader = OggStreamReader::new(reader)
            .map_err(|e| format!("{}", e))?;
        let sampling_rate = ogg_reader.ident_hdr.audio_sample_rate;

        Ok(plugin_commons::adapt_sampling_rate(OggAudioSource {
            reader: ogg_reader,
            remaining: VecDeque::new()
        }, sampling_rate))
    }
}

struct OggPlugin;

impl Plugin for OggPlugin {

    fn load_plugin(&self) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        vec![Box::new(OggAudioSourceResolver)]
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        Vec::new()
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        Vec::new()
    }

    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>> {
        Vec::new()
    }
}

fn make_ogg_plugin() -> OggPlugin {
    OggPlugin
}

rambot_api::export_plugin!(make_ogg_plugin);
