use rambot_api::audio::AudioSource;
use rambot_api::audio::convert::{
    self,
    MonoAudioSource,
    ResamplingAudioSource,
    StereoAudioSource
};

pub fn source_from_sample_rate<S>(s: S, sample_rate: u32)
    -> Box<dyn AudioSource + Send>
where
    S: AudioSource + Send + 'static
{
    let required = convert::REQUIRED_SAMPLING_RATE as u32;

    if sample_rate == required {
        Box::new(s)
    }
    else {
        Box::new(ResamplingAudioSource::new_to_required(s, sample_rate as f32))
    }
}

pub fn source_from_format<I>(iterator: I, channels: u16, sample_rate: u32)
    -> Result<Box<dyn AudioSource + Send>, String>
where
    I: Iterator<Item = f32> + Send + 'static
{
    match channels {
        1 => {
            let source = MonoAudioSource::new(iterator);
            Ok(source_from_sample_rate(source, sample_rate))
        },
        2 => {
            let source = StereoAudioSource::new(iterator);
            Ok(source_from_sample_rate(source, sample_rate))
        },
        _ => Err("Only mono and stereo files are supported.".to_owned())
    }
}
