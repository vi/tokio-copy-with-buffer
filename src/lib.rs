//! Customized variant of [`tokio_io::io::copy` function] where you can set your own
//! buffer and retrieve it after copying. This may increase performance in some cases.
//! [`tokio_io::io::copy` function]: https://docs.rs/tokio-io/0.1/tokio_io/io/fn.copy.html
#![deny(missing_docs)]

extern crate futures;
#[macro_use]
extern crate tokio_io;

use std::io;

use futures::{Future, Poll};

use tokio_io::{AsyncRead, AsyncWrite};

/// A future which will copy all data from a reader into a writer.
///
/// Created by the [`copy_with_buffer`] function, this future will resolve to the number of
/// bytes copied (along with other things) or an error if one happens.
///
/// [`copy_with_buffer`]: fn.copy_with_buffer.html
#[derive(Debug)]
pub struct Copy<R, W> {
    reader: Option<R>,
    read_done: bool,
    writer: Option<W>,
    pos: usize,
    cap: usize,
    amt: u64,
    buffer: Option<Box<[u8]>>,
}

/// Creates a future which represents copying all the bytes from one object to
/// another, just like [the original `copy`] function. This version uses large
/// buffer (65536) by default, unlike 4096 as in the original.
///
/// The returned future will copy all the bytes read from `reader` into the
/// `writer` specified. This future will only complete once the `reader` has hit
/// EOF and all bytes have been written to and flushed from the `writer`
/// provided.
///
/// On success the number of bytes is returned and the `reader` and `writer` are
/// consumed. Additionally the buffer used for copying is also returned.
/// On error the error is returned and the I/O objects are consumed as
/// well.
/// [the original `copy`]: https://docs.rs/tokio-io/0.1/tokio_io/io/fn.copy.html
pub fn copy<R, W>(reader: R, writer: W) -> Copy<R, W>
    where R: AsyncRead,
          W: AsyncWrite,
{
    copy_with_buffer(reader, writer, Box::new([0; 65536]))
}

/// Advanced version of [`copy`] where you can specify your own buffer.
/// Buffer may be reused for multiple copy operations.
///
/// For other description text see the [`copy` function documentation].
/// [`copy` function documentation]: fn.copy.html
pub fn copy_with_buffer<R, W>(reader: R, writer: W, buffer: Box<[u8]>) -> Copy<R, W>
    where R: AsyncRead,
          W: AsyncWrite,
{
    Copy {
        reader: Some(reader),
        read_done: false,
        writer: Some(writer),
        amt: 0,
        pos: 0,
        cap: 0,
        buffer: Some(buffer),
    }
}

impl<R, W> Future for Copy<R, W>
    where R: AsyncRead,
          W: AsyncWrite,
{
    type Item = (u64, R, W, Box<[u8]>);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(u64, R, W, Box<[u8]>), io::Error> {
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if self.pos == self.cap && !self.read_done {
                let buf = self.buffer.as_mut().unwrap();
                let reader = self.reader.as_mut().unwrap();
                let n = try_nb!(reader.read(buf));
                if n == 0 {
                    self.read_done = true;
                } else {
                    self.pos = 0;
                    self.cap = n;
                }
            }

            // If our buffer has some data, let's write it out!
            while self.pos < self.cap {
                let buf = self.buffer.as_mut().unwrap();
                let writer = self.writer.as_mut().unwrap();
                let i = try_nb!(writer.write(&mut buf[self.pos..self.cap]));
                if i == 0 {
                    return Err(io::Error::new(io::ErrorKind::WriteZero,
                                              "write zero byte into writer"));
                } else {
                    self.pos += i;
                    self.amt += i as u64;
                }
            }

            // If we've written al the data and we've seen EOF, flush out the
            // data and finish the transfer.
            // done with the entire transfer.
            if self.pos == self.cap && self.read_done {
                try_nb!(self.writer.as_mut().unwrap().flush());
                let reader = self.reader.take().unwrap();
                let writer = self.writer.take().unwrap();
                let buffer = self.buffer.take().unwrap();
                return Ok((self.amt, reader, writer, buffer).into())
            }
        }
    }
}
