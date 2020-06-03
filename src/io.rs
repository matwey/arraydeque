//! Buffered I/O based on `ArrayDeque`.

use std::io;
use std::io::BufRead;
use std::io::IoSliceMut;
use std::io::Read;

use crate::Array;
use crate::ArrayDeque;
use crate::behavior::Saturating;

struct Guard<'a, A> where A : Array {
    buf: &'a mut ArrayDeque<A, Saturating>,
    len: usize,
}

impl<A> Drop for Guard<'_, A> where A : Array {
    fn drop(&mut self) {
        unsafe {
            self.buf.set_len(self.len);
        }
    }
}

/// `std::io::BufReader` replacement driven by `ArrayDeque`.
pub struct BufReader<R, A> where A : Array<Item=u8> {
    inner: R,
    buf: ArrayDeque<A, Saturating>,
}

impl<R: Read, A : Array<Item=u8>> BufReader<R, A> {
    /// Creates a new BufReader<R, A>
    pub fn new(inner: R) -> BufReader<R, A> {
        BufReader { inner, buf: ArrayDeque::<A, Saturating>::new() }
    }
}

impl<R, A : Array<Item=u8>> BufReader<R, A> {
    /// Gets a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Unwraps this `BufReader<R, A>`, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Returns a reference to the internal deque.
    pub fn buffer(&self) -> &ArrayDeque::<A, Saturating> {
        &self.buf
    }

    /// Returns a mutable reference to the internal deque.
    pub fn buffer_mut(&mut self) -> &mut ArrayDeque::<A, Saturating> {
        &mut self.buf
    }
}

impl<R: Read, A : Array<Item=u8>> BufReader<R, A> {
    /// Tries to fill the internal deque from the internal reader
    ///
    /// Since this function tries to read from the internal reader if the deque
    /// has empty space, it may blocks.
    ///
    /// Returns the number of the read bytes. Returns 0 on EOF or when overflow
    /// is occurred. The latter can be distinguished by checking
    /// `self.buffer().is_full()`.
    ///
    /// # Errors
    /// Return an I/O error if it happens in the internal reader.
    ///
    /// If the I/O error is encountered then all bytes read so far will be
    /// present in buf and its length will have been adjusted appropriately.
    pub fn try_fill_buf(&mut self) -> io::Result<usize> {
        if self.buf.is_full() {
            return Ok(0);
        }

        let empty_pos = self.buf.len();

        let mut g = Guard { buf: &mut self.buf, len: empty_pos };
        unsafe {
            g.buf.set_len(g.buf.capacity());
        }

        let bufs = g.buf.as_mut_slices();
        let (advance1, advance2) = match (bufs.0.len(), bufs.1.len()) {
            (len, _) if empty_pos < len =>
                (empty_pos, 0),
            (len1, len2) if empty_pos >= len1 && empty_pos - len1 < len2 =>
                (len1, empty_pos - len1),
            x => x,
        };

        let mut bufs = [IoSliceMut::new(&mut bufs.0[advance1..]), IoSliceMut::new(&mut bufs.1[advance2..])];
        let bytes = self.inner.read_vectored(&mut bufs)?;
        g.len += bytes;
        drop(g);

        Ok(bytes)
    }
}

impl<R: Read, A : Array<Item=u8>> Read for BufReader<R, A> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut ibuf = self.fill_buf()?;
        let nread = ibuf.read(buf)?;
        self.consume(nread);
        Ok(nread)
    }
}

impl<R: Read, A : Array<Item=u8>> BufRead for BufReader<R, A> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.buf.is_empty() {
            self.try_fill_buf()?;
        }

        Ok(self.buf.as_slices().0)
    }

    fn consume(&mut self, amt: usize) {
        self.buf.drain(0..amt);
    }
}
