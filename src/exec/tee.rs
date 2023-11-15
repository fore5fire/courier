use std::ops::DerefMut;
use std::pin::Pin;

use tokio::io::{self, AsyncRead, AsyncWrite};

pub(crate) struct Tee<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> {
    inner: T,
    pub reads: Vec<u8>,
    pub writes: Vec<u8>,
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Tee<T> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Tee<T> {
    pub fn new(wrap: T) -> Self {
        Tee {
            inner: wrap,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> AsyncRead for Tee<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let old_len = buf.filled().len();
        let poll = Pin::new(&mut self.deref_mut().inner).poll_read(cx, buf);
        self.reads.extend_from_slice(&buf.filled()[old_len..]);
        poll
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> AsyncWrite for Tee<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let poll = Pin::new(&mut self.deref_mut().inner).poll_write(cx, buf);
        if poll.is_ready() {
            self.get_mut().writes.extend_from_slice(&buf);
        }
        poll
    }
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.deref_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.deref_mut().inner).poll_shutdown(cx)
    }
}