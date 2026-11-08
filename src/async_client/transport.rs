//! Native async ISO transport using the same pure wire validation as blocking I/O.

use crate::{
    Error, Result,
    transport::{
        TransportConfig, connection_request, frame_bytes, frame_payload_len, parse_confirm,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, timeout_at},
};

/// Socket ownership is moved out of the session during every awaited exchange.
#[derive(Debug)]
pub(super) struct AsyncConnection {
    stream: TcpStream,
    tpdu_size: usize,
    peer: u16,
    max_message: usize,
}

impl AsyncConnection {
    /// Negotiate COTP over a nonblocking Tokio socket under one deadline.
    pub(super) async fn connect(config: &TransportConfig, deadline: Instant) -> Result<Self> {
        config.validate()?;
        let stream = timeout_at(deadline, TcpStream::connect(config.endpoint))
            .await
            .map_err(|_| Error::Timeout)??;
        stream.set_nodelay(true)?;
        let mut connection = Self {
            stream,
            tpdu_size: 1usize << config.tpdu_exponent,
            peer: 0,
            max_message: config.max_message_size,
        };
        connection
            .write_frame(&connection_request(config)?, deadline)
            .await?;
        let reply = connection.read_frame(deadline).await?;
        let (peer, tpdu_size) = parse_confirm(&reply, connection.tpdu_size)?;
        connection.peer = peer;
        connection.tpdu_size = tpdu_size;
        Ok(connection)
    }

    /// Fragment and send a message without blocking executor threads.
    pub(super) async fn send(&mut self, data: &[u8], deadline: Instant) -> Result<()> {
        if data.len() > self.max_message {
            return Err(Error::InvalidInput("message exceeds transport limit"));
        }
        if data.is_empty() {
            return self.write_frame(&[2, 0xf0, 0x80], deadline).await;
        }
        let chunks = data.chunks(self.tpdu_size - 3);
        let count = chunks.len();
        for (index, bytes) in chunks.enumerate() {
            let mut payload = vec![2, 0xf0, if index + 1 == count { 0x80 } else { 0 }];
            payload.extend_from_slice(bytes);
            self.write_frame(&payload, deadline).await?;
        }
        Ok(())
    }

    /// Reassemble a bounded COTP message using the caller's original deadline.
    pub(super) async fn receive(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        for _ in 0..1024 {
            let frame = self.read_frame(deadline).await?;
            if frame.len() < 3
                || frame.len() > self.tpdu_size
                || frame[0] != 2
                || frame[1] != 0xf0
                || frame[2] & 0x7f != 0
            {
                return Err(Error::Protocol("invalid COTP data fragment"));
            }
            if result.len() + frame.len() - 3 > self.max_message {
                return Err(Error::Protocol("reassembled message exceeds limit"));
            }
            result.extend_from_slice(&frame[3..]);
            if frame[2] & 0x80 != 0 {
                return Ok(result);
            }
            if frame.len() == 3 {
                return Err(Error::Protocol("empty continuation fragment"));
            }
        }
        Err(Error::Protocol("too many COTP fragments"))
    }

    /// Send the correct peer reference, then close even on error or cancellation.
    pub(super) async fn disconnect(mut self, deadline: Instant) -> Result<()> {
        let peer = self.peer.to_be_bytes();
        self.write_frame(&[6, 0x80, peer[0], peer[1], 0, 1, 0], deadline)
            .await?;
        timeout_at(deadline, self.stream.shutdown())
            .await
            .map_err(|_| Error::Timeout)??;
        Ok(())
    }

    /// Keep the header and payload under a single absolute async deadline.
    async fn read_frame(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        timeout_at(deadline, async {
            let mut header = [0; 4];
            self.stream.read_exact(&mut header).await?;
            let mut payload = vec![0; frame_payload_len(header)?];
            self.stream.read_exact(&mut payload).await?;
            Ok(payload)
        })
        .await
        .map_err(|_| Error::Timeout)?
    }

    /// Validate lengths before writing and bound all partial writes together.
    async fn write_frame(&mut self, payload: &[u8], deadline: Instant) -> Result<()> {
        let frame = frame_bytes(payload)?;
        timeout_at(deadline, self.stream.write_all(&frame))
            .await
            .map_err(|_| Error::Timeout)??;
        Ok(())
    }
}
