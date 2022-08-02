use crate::util::RightPaddedAudioSource;

use rambot_api::{AudioSource, Sample};

use std::f32::consts;
use std::io;

#[cfg(all(target_arch = "x86", target_feature = "sse"))]
use std::arch::x86::{
    __m128,
    _mm_add_ps,
    _mm_cvtss_f32,
    _mm_loadu_ps,
    _mm_mul_ps,
    _mm_setzero_ps,
    _mm_shuffle_ps
};

#[cfg(all(target_arch = "x86_64", target_feature = "sse"))]
use std::arch::x86_64::{
    __m128,
    _mm_add_ps,
    _mm_cvtss_f32,
    _mm_loadu_ps,
    _mm_mul_ps,
    _mm_setzero_ps,
    _mm_shuffle_ps
};

#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse"))]
#[inline]
unsafe fn sum_ps(mut a: __m128) -> f32 {
    let mut shuffled = _mm_shuffle_ps::<0xb1>(a, a);
    a = _mm_add_ps(a, shuffled);
    shuffled = _mm_shuffle_ps::<0x1b>(a, a);
    a = _mm_add_ps(a, shuffled);
    _mm_cvtss_f32(a)
}

#[allow(unused_mut)] // if compiled without SSE, "mut" modifiers are unused
fn fold(mut a: &[Sample], mut b: &[f32]) -> Sample {
    let mut sum = Sample::ZERO;

    #[cfg(all(any(target_arch = "x86", target_arch = "x86_64"),
        target_feature = "sse"))]
    unsafe {
        let sse_steps = b.len() / 4;
        let mut left = _mm_setzero_ps();
        let mut right = _mm_setzero_ps();
        let mut a_ptr = a.as_ptr() as *const f32;
        let mut b_ptr = b.as_ptr();

        for _ in 0..sse_steps {
            let a_1 = _mm_loadu_ps(a_ptr);
            let a_2 = _mm_loadu_ps(a_ptr.add(4));
            let a_left = _mm_shuffle_ps::<0xdd>(a_1, a_2);
            let a_right = _mm_shuffle_ps::<0x88>(a_1, a_2);
            let b = _mm_loadu_ps(b_ptr);

            left = _mm_add_ps(_mm_mul_ps(a_left, b), left);
            right = _mm_add_ps(_mm_mul_ps(a_right, b), right);

            a_ptr = a_ptr.add(8);
            b_ptr = b_ptr.add(4);
        }

        sum.left = sum_ps(left);
        sum.right = sum_ps(right);

        a = &a[sse_steps * 2..];
        b = &b[sse_steps * 4..];
    }

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

        for (i, sample) in buf.iter_mut().enumerate().take(count) {
            *sample = fold(&self.buf[i..(i + kernel_size)], &self.kernel);
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

const SQRT_TAU: f32 = (2.0 / consts::FRAC_2_SQRT_PI) * consts::SQRT_2;

pub(crate) fn gaussian(sigma: f32, kernel_size_sigmas: f32) -> Vec<f32> {
    let kernel_size = (kernel_size_sigmas * sigma).ceil() as usize * 2 + 1;
    let mut kernel = Vec::with_capacity(kernel_size);
    let mid_point = (kernel_size / 2) as isize;

    for i in 0..kernel_size {
        let x = (i as isize - mid_point) as f32;
        let exponent = -(x * x / (2.0 * sigma * sigma));
        kernel.push(1.0 / (SQRT_TAU * sigma) * consts::E.powf(exponent));
    }

    kernel
}

pub(crate) fn inv_gaussian(sigma: f32, kernel_size_sigmas: f32) -> Vec<f32> {
    let mut kernel = gaussian(sigma, kernel_size_sigmas);

    for f in kernel.iter_mut() {
        *f = -*f;
    }

    let mid_point = kernel.len() / 2;
    kernel[mid_point] += 1.0;

    kernel
}
