use crate::util::RightPaddedAudioSource;

use rambot_api::{AudioSource, Sample};

use std::f32::consts;
use std::io;

fn fold(a: &[Sample], b: &[f32]) -> Sample {
    let mut sum = Sample::ZERO;

    for (s, &f) in a.iter().zip(b.iter()) {
        sum += s * f;
    }

    sum
}

pub(crate) struct KernelFilter {
    child: RightPaddedAudioSource,
    kernel: Vec<f32>,
    buf: Vec<Sample>
}

impl KernelFilter {
    pub(crate) fn new(child: Box<dyn AudioSource + Send>, kernel: Vec<f32>)
            -> KernelFilter {
        let padding = kernel.len() - 1;
        let child = RightPaddedAudioSource::new(child, padding);
        let buf = vec![Sample::ZERO; padding];

        KernelFilter {
            child,
            kernel,
            buf
        }
    }
}

impl AudioSource for KernelFilter {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let kernel_size = self.kernel.len();
        let required_buf_len = buf.len() + kernel_size - 1;

        if self.buf.len() < required_buf_len {
            self.buf.append(
                &mut vec![Sample::ZERO; required_buf_len - self.buf.len()]);
        }

        let count = self.child.read(
            &mut self.buf[(kernel_size - 1)..required_buf_len])?;

        for i in 0..count {
            buf[i] = fold(&self.buf[i..(i + kernel_size)], &self.kernel);
        }

        self.buf.copy_within(count..(count + kernel_size - 1), 0);
        Ok(count)
    }

    fn has_child(&self) -> bool {
        true
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        self.child.take_child()
    }
}

const KERNEL_SIZE_STD_DEVIATIONS: f32 = 5.0;
const SQRT_TAU: f32 = (2.0 / consts::FRAC_2_SQRT_PI) * consts::SQRT_2;

pub(crate) fn gaussian(sigma: f32) -> Vec<f32> {
    let kernel_size =
        (KERNEL_SIZE_STD_DEVIATIONS * sigma).ceil() as usize * 2 + 1;
    let mut kernel = Vec::with_capacity(kernel_size);
    let mid_point = (kernel_size / 2) as isize;

    for i in 0..kernel_size {
        let x = (i as isize - mid_point) as f32;
        let exponent = -(x * x / (2.0 * sigma * sigma));
        kernel.push(1.0 / (SQRT_TAU * sigma) * consts::E.powf(exponent));
    }

    kernel
}

pub(crate) fn inv_gaussian(sigma: f32) -> Vec<f32> {
    let mut kernel = gaussian(sigma);

    for f in kernel.iter_mut() {
        *f = -*f;
    }

    let mid_point = kernel.len() / 2;
    kernel[mid_point] += 1.0;

    kernel
}
