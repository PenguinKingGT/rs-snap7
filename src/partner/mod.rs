//! Active/passive PBC Partner sessions with one receive dispatcher.
//!
//! Sends are serialized while incoming pushes are always dispatched and ACKed,
//! allowing simultaneous sends in both directions. No ambiguous send is replayed.

pub mod wire;
use crate::{
    Error, Result,
    protocol::{self, Pdu, PduType},
    transport::{Connection, TransportConfig, accept_connection},
};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU16, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Completion sink; application data is omitted from events.
pub type PartnerHook = Arc<dyn Fn(&PartnerEvent) + Send + Sync>;

/// Bounded Partner settings.
#[derive(Clone)]
pub struct PartnerConfig {
    /// Endpoint, TSAPs and per-wire-operation timeout.
    pub transport: TransportConfig,
    /// Requested application PDU, at least 128.
    pub pdu_length: u16,
    /// Received transfers buffered before disconnecting a slow consumer.
    pub queue_capacity: usize,
    /// Completion sink; invoked outside internal locks.
    pub callback: Option<PartnerHook>,
}
impl PartnerConfig {
    /// Standard active settings; passive callers set transport.local_tsap explicitly.
    pub fn new(endpoint: SocketAddr) -> Self {
        Self {
            transport: TransportConfig::new(endpoint),
            pdu_length: 480,
            queue_capacity: 32,
            callback: None,
        }
    }
    /// Check resource and framing limits before binding/connecting.
    fn validate(&self) -> Result<()> {
        self.transport.validate()?;
        if self.pdu_length < 128
            || self.pdu_length as usize > self.transport.max_message_size
            || !(1..=1024).contains(&self.queue_capacity)
        {
            return Err(Error::InvalidInput("invalid Partner limits"));
        }
        Ok(())
    }
}
/// One received transfer with its unmodified request identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// Peer-supplied R-ID.
    pub r_id: u32,
    /// Application bytes.
    pub data: Vec<u8>,
}
/// Completion notification; callbacks must not block or synchronously stop the dispatcher.
#[derive(Debug, Clone)]
pub enum PartnerEvent {
    /// An outgoing transfer was acknowledged.
    Sent {
        /// Application bytes.
        bytes: usize,
    },
    /// An incoming transfer was queued and acknowledged.
    Received {
        /// Peer-supplied R-ID.
        r_id: u32,
        /// Application bytes.
        bytes: usize,
    },
    /// Session terminated; no application data is included.
    Failed,
}
/// Partner lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Session can exchange transfers.
    Connected,
    /// Explicitly stopped.
    Stopped,
    /// Dispatcher failed; construct a fresh session to reconnect.
    Failed,
}
/// Cumulative counters and most recent logical operation timings.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Positively acknowledged outgoing bytes.
    pub bytes_sent: u64,
    /// Queued and acknowledged incoming bytes.
    pub bytes_received: u64,
    /// Acknowledged outgoing transfers.
    pub sends: u64,
    /// Queued incoming transfers.
    pub receives: u64,
    /// Most recent successful send duration.
    pub last_send: Duration,
    /// Most recent successful receive wait.
    pub last_receive: Duration,
    /// Last diagnostic category/text, without data.
    pub last_error: Option<String>,
}
/// At most one outgoing transfer waits for its own ACK.
struct Pending {
    reference: u16,
    result: mpsc::SyncSender<Result<()>>,
}
/// Shared dispatcher state, independent of the owning lifetime handle.
struct State {
    writer: Mutex<Connection>,
    send_guard: Mutex<()>,
    pending: Mutex<Option<Pending>>,
    incoming: Mutex<mpsc::Receiver<Transfer>>,
    status: AtomicU8,
    reference: AtomicU16,
    stats: Mutex<Stats>,
    config: PartnerConfig,
    pdu: usize,
}
impl State {
    /// Invoke callbacks with no transport/state locks held.
    fn emit(&self, event: PartnerEvent) {
        if let Some(callback) = &self.config.callback {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&event)));
        }
    }
    /// Close both socket handles and wake a waiting sender with a definite failure.
    fn fail(&self, error: Error) {
        if self
            .status
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if let Ok(mut stats) = self.stats.lock() {
                stats.last_error = Some(error.to_string());
            }
            if let Ok(mut writer) = self.writer.lock() {
                writer.abort();
            }
            if let Ok(mut pending) = self.pending.lock()
                && let Some(pending) = pending.take()
            {
                let _ = pending.result.try_send(Err(error));
            }
            self.emit(PartnerEvent::Failed);
        }
    }
}
/// Lifetime owner whose final drop terminates its receive dispatcher.
struct Handle {
    state: Arc<State>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Handle {
    /// Wake blocked I/O before joining, without requiring a runtime.
    fn drop(&mut self) {
        self.state.status.store(1, Ordering::Release);
        if let Ok(mut writer) = self.state.writer.lock() {
            writer.abort();
        }
        if let Ok(worker) = self.worker.get_mut()
            && let Some(worker) = worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}

/// A connected Partner. Clones share serialization, queues and statistics.
#[derive(Clone)]
pub struct Partner {
    inner: Arc<Handle>,
}
impl Partner {
    /// Initiate TCP/COTP/S7 setup and start a sole receive dispatcher.
    pub fn connect(config: PartnerConfig) -> Result<Self> {
        config.validate()?;
        let mut connection = Connection::connect(&config.transport)?;
        let reply = Pdu::decode(
            &connection.exchange(&protocol::setup_request(1, config.pdu_length)?.encode()?)?,
        )?;
        let pdu = protocol::parse_setup(&reply, config.pdu_length)?.pdu_length;
        Self::start(connection, config, pdu as usize)
    }
    /// Start a connected session after active/passive setup.
    fn start(connection: Connection, config: PartnerConfig, pdu: usize) -> Result<Self> {
        let mut reader = connection.reader_clone()?;
        let (sender, incoming) = mpsc::sync_channel(config.queue_capacity);
        let state = Arc::new(State {
            writer: Mutex::new(connection),
            send_guard: Mutex::new(()),
            pending: Mutex::new(None),
            incoming: Mutex::new(incoming),
            status: AtomicU8::new(0),
            reference: AtomicU16::new(2),
            stats: Default::default(),
            config,
            pdu,
        });
        let dispatch = state.clone();
        let worker = thread::Builder::new()
            .name("snap7-partner".into())
            .spawn(move || {
                while dispatch.status.load(Ordering::Acquire) == 0 {
                    let result = (|| {
                        let bytes = reader.receive()?;
                        if bytes.len() > dispatch.pdu {
                            return Err(Error::Protocol("Partner PDU limit exceeded"));
                        }
                        let pdu = Pdu::decode(&bytes)?;
                        if pdu.parameters.get(5) == Some(&0x46) {
                            let (r_id, data) = wire::parse_data(&pdu)?;
                            let size = data.len();
                            sender.try_send(Transfer { r_id, data }).map_err(|_| {
                                Error::Protocol("Partner receive queue full or closed")
                            })?;
                            dispatch
                                .writer
                                .lock()
                                .map_err(|_| Error::Disconnected)?
                                .send(&wire::ack(pdu.reference).encode()?)?;
                            if let Ok(mut stats) = dispatch.stats.lock() {
                                stats.bytes_received += size as u64;
                                stats.receives += 1;
                            }
                            dispatch.emit(PartnerEvent::Received { r_id, bytes: size });
                        } else {
                            let pending = dispatch
                                .pending
                                .lock()
                                .map_err(|_| Error::Disconnected)?
                                .take()
                                .ok_or(Error::Protocol("unsolicited Partner ACK"))?;
                            let result = wire::check_ack(&pdu, pending.reference);
                            let fatal = result.is_err();
                            let _ = pending.result.try_send(result);
                            if fatal {
                                return Err(Error::Protocol("Partner ACK rejected"));
                            }
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        dispatch.fail(error);
                        break;
                    }
                }
            })?;
        Ok(Self {
            inner: Arc::new(Handle {
                state,
                worker: Mutex::new(Some(worker)),
            }),
        })
    }
    /// Send one bounded push and wait for its matching ACK.
    /// Lost ACKs report an ambiguous operation, with no automatic retry.
    pub fn send(&self, r_id: u32, data: &[u8]) -> Result<()> {
        let state = &self.inner.state;
        wire::data(0, r_id, data, state.pdu)?;
        let started = Instant::now();
        let _guard = state.send_guard.lock().map_err(|_| Error::Disconnected)?;
        if self.status() != Status::Connected {
            return Err(Error::Disconnected);
        }
        let reference = state.reference.fetch_add(1, Ordering::Relaxed);
        let job = wire::data(reference, r_id, data, state.pdu)?.encode()?;
        let (sender, receiver) = mpsc::sync_channel(1);
        *state.pending.lock().map_err(|_| Error::Disconnected)? = Some(Pending {
            reference,
            result: sender,
        });
        // Release the writer before waiting: the dispatcher must ACK incoming pushes.
        let sent = {
            state
                .writer
                .lock()
                .map_err(|_| Error::Disconnected)?
                .send(&job)
        };
        let result = sent.and_then(|_| {
            receiver
                .recv_timeout(state.config.transport.timeout)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => Error::Timeout,
                    _ => Error::Disconnected,
                })?
        });
        if let Err(error) = result {
            let text = error.to_string();
            drop(_guard);
            state.fail(Error::Protocol("Partner send failed"));
            if let Ok(mut stats) = state.stats.lock() {
                stats.last_error = Some(text);
            }
            return Err(Error::AmbiguousOperation {
                operation: "Partner send",
                source: Box::new(error),
            });
        }
        if let Ok(mut stats) = state.stats.lock() {
            stats.bytes_sent += data.len() as u64;
            stats.sends += 1;
            stats.last_send = started.elapsed();
        }
        drop(_guard);
        state.emit(PartnerEvent::Sent { bytes: data.len() });
        Ok(())
    }
    /// Wait for a received push. A queue timeout leaves the session usable.
    pub fn receive(&self, timeout: Duration) -> Result<Transfer> {
        if timeout.is_zero() {
            return Err(Error::InvalidInput("zero Partner receive timeout"));
        }
        let started = Instant::now();
        let result = self
            .inner
            .state
            .incoming
            .lock()
            .map_err(|_| Error::Disconnected)?
            .recv_timeout(timeout)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => Error::Timeout,
                _ => Error::Disconnected,
            });
        if result.is_ok()
            && let Ok(mut stats) = self.inner.state.stats.lock()
        {
            stats.last_receive = started.elapsed();
        }
        result
    }
    /// Snapshot lifecycle state.
    pub fn status(&self) -> Status {
        match self.inner.state.status.load(Ordering::Acquire) {
            0 => Status::Connected,
            1 => Status::Stopped,
            _ => Status::Failed,
        }
    }
    /// Snapshot counters without exposing locks.
    pub fn stats(&self) -> Result<Stats> {
        Ok(self
            .inner
            .state
            .stats
            .lock()
            .map_err(|_| Error::Disconnected)?
            .clone())
    }
    /// Stop all clones, closing transport and waking pending send/receive operations.
    pub fn stop(&self) {
        let state = &self.inner.state;
        state.status.store(1, Ordering::Release);
        if let Ok(mut writer) = state.writer.lock() {
            writer.abort();
        }
        if let Ok(mut pending) = state.pending.lock()
            && let Some(pending) = pending.take()
        {
            let _ = pending.result.try_send(Err(Error::Disconnected));
        }
    }

    /// Run blocking Partner sending on Tokio's blocking pool.
    /// Cancellation detaches the task; an already started send continues to completion.
    #[cfg(feature = "async")]
    pub async fn send_async(&self, r_id: u32, data: Vec<u8>) -> Result<()> {
        let partner = self.clone();
        tokio::task::spawn_blocking(move || partner.send(r_id, &data))
            .await
            .map_err(|_| Error::Disconnected)?
    }
    /// Wait on Tokio's blocking pool; cancellation detaches a bounded receive wait.
    #[cfg(feature = "async")]
    pub async fn receive_async(&self, timeout: Duration) -> Result<Transfer> {
        let partner = self.clone();
        tokio::task::spawn_blocking(move || partner.receive(timeout))
            .await
            .map_err(|_| Error::Disconnected)?
    }
}

/// Passive listener; accepting one peer has the configured total accept deadline.
pub struct PartnerListener {
    listener: TcpListener,
    config: PartnerConfig,
}
impl PartnerListener {
    /// Bind a passive endpoint; transport.local_tsap must match the caller's remote TSAP.
    pub fn bind(config: PartnerConfig) -> Result<Self> {
        config.validate()?;
        let listener = TcpListener::bind(config.transport.endpoint)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, config })
    }
    /// Actual bound address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }
    /// Accept COTP/setup and return a usable bidirectional session.
    pub fn accept(&self) -> Result<Partner> {
        let deadline = Instant::now() + self.config.transport.timeout;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(Error::Timeout);
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mut connection = accept_connection(
            stream,
            self.config.transport.timeout,
            self.config.transport.max_message_size,
            Some(&self.config.transport.local_tsap.to_be_bytes()),
        )?;
        let job = Pdu::decode(&connection.receive()?)?;
        if job.kind != PduType::Job
            || job.parameters.len() != 8
            || job.parameters[..2] != [0xf0, 0]
            || !job.data.is_empty()
        {
            return Err(Error::Protocol("Partner requires setup communication"));
        }
        let requested = crate::codec::read::<u16>(&job.parameters, 6)?;
        if requested < 128 {
            return Err(Error::Protocol("invalid Partner requested PDU"));
        }
        let pdu = requested.min(self.config.pdu_length);
        let mut params = vec![0xf0, 0, 0, 1, 0, 1];
        params.extend_from_slice(&pdu.to_be_bytes());
        let response = Pdu {
            kind: PduType::AckData,
            reference: job.reference,
            error: 0,
            parameters: params,
            data: vec![],
        };
        connection.send(&response.encode()?)?;
        Partner::start(connection, self.config.clone(), pdu as usize)
    }
}
