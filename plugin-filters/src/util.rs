use rambot_api::{AudioSource, Sample};

use std::io;

pub(crate) struct RightPaddedAudioSource {
    padding: usize,
    child: Option<Box<dyn AudioSource + Send>>,
    child_finished: bool
}

impl RightPaddedAudioSource {
    pub(crate) fn new(child: Box<dyn AudioSource + Send>, padding: usize)
            -> RightPaddedAudioSource {
        RightPaddedAudioSource {
            padding,
            child: Some(child),
            child_finished: false
        }
    }
}

impl AudioSource for RightPaddedAudioSource {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        if !self.child_finished {
            let child_count = self.child.as_mut().unwrap().read(buf)?;

            if child_count == 0 {
                self.child_finished = true;
            }
            else {
                return Ok(child_count);
            }
        }

        let zeros = self.padding.min(buf.len());

        for i in 0..zeros {
            buf[i] = Sample::ZERO;
        }
    
        self.padding -= zeros;
        Ok(zeros)
    }

    fn has_child(&self) -> bool {
        true
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        self.child.take().unwrap()
    }
}
