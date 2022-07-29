use std::io::{self, Read, Seek, SeekFrom};
use std::slice;

const INITIAL_CAPACITY: usize = 16384; // 16k

/// A wrapper around a type implementing [Read] that also implements [Seek] by
/// storing all bytes that were read. Using this introduces significant
/// overhead in terms of memory, but also in terms of runtime, as reading now
/// requires copying from the internal buffer into the output buffer.
pub struct SeekWrapper<R> {
    read: R,
    buffer: Vec<u8>,
    offset: usize,
    finished: bool
}

impl<R> SeekWrapper<R> {

    /// Creates a new seek wrapper around the given reader.
    pub fn new(read: R) -> SeekWrapper<R> {
        SeekWrapper {
            read,
            buffer: Vec::with_capacity(INITIAL_CAPACITY),
            offset: 0,
            finished: false
        }
    }
}

impl<R: Read> SeekWrapper<R> {
    fn fill_buffer(&mut self) -> io::Result<()> {
        if self.buffer.len() >= self.buffer.capacity() {
            self.buffer.reserve(self.buffer.capacity());
        }

        let buf = unsafe {
            let spare_capacity = self.buffer.spare_capacity_mut();
            let buf_ptr = spare_capacity.as_mut_ptr() as *mut u8;

            slice::from_raw_parts_mut(buf_ptr, spare_capacity.len())
        };

        let count = self.read.read(buf)?;

        if count > buf.len() {
            panic!("Underlying read implementation is misbehaving.");
        }

        unsafe { self.buffer.set_len(self.buffer.len() + count); }
        self.finished |= count == 0;
        Ok(())
    }
}

impl<R: Read> Read for SeekWrapper<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.offset >= self.buffer.len() && !self.finished {
            self.fill_buffer()?;
        }

        if self.offset >= self.buffer.len() && self.finished {
            return Ok(0);
        }

        let count = (self.buffer.len() - self.offset).min(buf.len());
        buf[..count].copy_from_slice(
            &self.buffer[self.offset..(self.offset + count)]);
        self.offset += count;

        Ok(count)
    }
}

impl<R: Read> Seek for SeekWrapper<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Start(offset) => self.offset = offset as usize,
            SeekFrom::End(offset) => {
                while !self.finished {
                    self.fill_buffer()?;
                }

                self.offset = (self.buffer.len() as i64 + offset) as usize;
            },
            SeekFrom::Current(delta) =>
                self.offset = (self.offset as i64 + delta) as usize
        }

        Ok(self.offset as u64)
    }
}
