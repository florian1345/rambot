use rambot_api::{AudioMetadata, AudioSource, Sample, SampleDuration};

use std::io;

pub(crate) struct EchoEffect {
    child: Option<Box<dyn AudioSource + Send + Sync>>,
    history: Vec<Sample>,
    factor: f32
}

impl EchoEffect {

    /// Returns `Ok(effect)` if `delay` is valid (positive and not too large),
    /// and `Err(child)` otherwise.
    pub(crate) fn new(child: Box<dyn AudioSource + Send + Sync>,
            delay: SampleDuration, factor: f32)
            -> Result<EchoEffect, Box<dyn AudioSource + Send + Sync>> {
        if delay > SampleDuration::ZERO {
            if let Ok(history_len) = usize::try_from(delay.samples()) {
                return Ok(EchoEffect {
                    child: Some(child),
                    history: vec![Sample::ZERO; history_len],
                    factor
                });
            }
        }

        Err(child)
    }
}

impl AudioSource for EchoEffect {
    fn read(&mut self, mut buf: &mut [Sample]) -> Result<usize, io::Error> {
        let mut total = 0;
        let history_len = self.history.len();

        while !buf.is_empty() {
            let max = buf.len().min(self.history.len());
            let count = self.child.as_mut().unwrap().read(&mut buf[..max])?;

            for (i, sample) in buf[..count].iter_mut().enumerate() {
                *sample += self.history[i] * self.factor;
            }

            self.history.copy_within(count.., 0);
            self.history[(history_len - count)..]
                .copy_from_slice(&buf[..count]);

            total += count;
            buf = &mut buf[count..];

            if count < max {
                break;
            }
        }

        Ok(total)
    }

    fn has_child(&self) -> bool {
        true
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
        self.child.take().unwrap()
    }

    fn metadata(&self) -> AudioMetadata {
        self.child.as_ref().unwrap().metadata()
    }
}
