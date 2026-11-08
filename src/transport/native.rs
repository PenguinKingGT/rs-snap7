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
pub(crate) struct AsyncConnection {
    stream: TcpStream,
    tpdu_size: usize,
    peer: u16,
    max_message: usize,
}

impl AsyncConnection {
    /// Negotiate COTP over a nonblocking Tokio socket under one deadline.
    pub(crate) async fn connect(config: &TransportConfig, deadline: Instant) -> Result<Self> {
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

    /// Negotiated maximum COTP frame length.
    pub(crate) fn tpdu_size(&self) -> usize {
        self.tpdu_size
    }

    /// Fragment and send a message without blocking executor threads.
    pub(crate) async fn send(&mut self, data: &[u8], deadline: Instant) -> Result<()> {
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
    pub(crate) async fn receive(&mut self, deadline: Instant) -> Result<Vec<u8>> {
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
    pub(crate) async fn disconnect(mut self, deadline: Instant) -> Result<()> {
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

/// Advanced asynchronous COTP connection without S7 negotiation.
/// Each operation uses one total deadline; errors and cancellation close the socket.
#[derive(Debug)]
pub struct Connection {
    connection: Option<AsyncConnection>,
    timeout: std::time::Duration,
    tpdu_size: usize,
}
impl Connection {
    /// Validate settings and negotiate COTP on the caller's Tokio runtime.
    pub async fn connect(config: &TransportConfig) -> Result<Self> {
        config.validate()?;
        let connection = AsyncConnection::connect(config, Instant::now() + config.timeout).await?;
        let tpdu_size = connection.tpdu_size();
        Ok(Self {
            connection: Some(connection),
            timeout: config.timeout,
            tpdu_size,
        })
    }
    /// Inspect local readiness without probing the peer.
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    /// Return the negotiated TPDU size in bytes.
    pub fn tpdu_size(&self) -> usize {
        self.tpdu_size
    }
    /// Send one bounded message; cancellation invalidates the connection.
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut connection = self.connection.take().ok_or(Error::Disconnected)?;
        connection.send(data, Instant::now() + self.timeout).await?;
        self.connection = Some(connection);
        Ok(())
    }
    /// Receive one reassembled message under a total deadline.
    pub async fn receive(&mut self) -> Result<Vec<u8>> {
        let mut connection = self.connection.take().ok_or(Error::Disconnected)?;
        let reply = connection.receive(Instant::now() + self.timeout).await?;
        self.connection = Some(connection);
        Ok(reply)
    }
    /// Send and receive under one shared deadline; failures discard the socket.
    pub async fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        let mut connection = self.connection.take().ok_or(Error::Disconnected)?;
        let deadline = Instant::now() + self.timeout;
        connection.send(request, deadline).await?;
        let reply = connection.receive(deadline).await?;
        self.connection = Some(connection);
        Ok(reply)
    }
    /// Send COTP disconnect and release the socket, including on cancellation.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(connection) = self.connection.take() {
            connection.disconnect(Instant::now() + self.timeout).await?;
        }
        Ok(())
    }
}
