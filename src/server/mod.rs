//! Bounded, threaded S7 emulator with registered memory and configurable services.
//!
//! This is a test/simulation server. It does not execute PLC programs or model
//! real CPU scheduling, protection, compression, or persistent ROM.

mod engine;
use crate::{Area, CpuState, Error, Result, codec::datetime::DateTime};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Server event categories; callbacks run after memory and queue locks are released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// Client accepted after admission checks.
    Connected,
    /// Client session ended.
    Disconnected,
    /// Read request completed.
    Read,
    /// Write request completed.
    Write,
    /// CPU/control service executed.
    Control,
    /// Protocol request rejected or client I/O failed.
    Error,
}
/// Minimal event metadata; application data and passwords are never included.
#[derive(Debug, Clone)]
pub struct Event {
    /// Event category.
    pub kind: EventKind,
    /// Remote endpoint.
    pub peer: SocketAddr,
    /// Function code, zero for connection events.
    pub function: u8,
}
/// Optional callback invoked outside emulator locks. Panics are isolated.
pub type EventHook = Arc<dyn Fn(&Event) + Send + Sync>;

/// Emulator configuration, validated before binding a socket.
#[derive(Clone)]
pub struct ServerConfig {
    /// Local interface/port; port zero asks the OS for an ephemeral port.
    pub bind: SocketAddr,
    /// Maximum simultaneous clients, 1–256.
    pub max_clients: usize,
    /// Maximum negotiated application PDU, 128–65535.
    pub pdu_length: u16,
    /// Per-transport deadline, including idle receives.
    pub timeout: Duration,
    /// Bounded queued event count; oldest events are dropped on overflow.
    pub event_capacity: usize,
    /// Categories recorded/dispatched; empty disables events.
    pub events: Vec<EventKind>,
    /// Optional general event sink.
    pub hook: Option<EventHook>,
}
impl ServerConfig {
    /// Construct a bounded emulator on a chosen endpoint.
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            max_clients: 32,
            pdu_length: 480,
            timeout: Duration::from_secs(5),
            event_capacity: 1024,
            events: vec![
                EventKind::Connected,
                EventKind::Disconnected,
                EventKind::Read,
                EventKind::Write,
                EventKind::Control,
                EventKind::Error,
            ],
            hook: None,
        }
    }
    /// Reject unreasonable resource bounds before network I/O.
    fn validate(&self) -> Result<()> {
        if !(1..=256).contains(&self.max_clients)
            || self.pdu_length < 128
            || self.timeout.is_zero()
            || std::time::Instant::now()
                .checked_add(self.timeout)
                .is_none()
            || !(1..=100_000).contains(&self.event_capacity)
        {
            return Err(Error::InvalidInput("invalid server limits"));
        }
        Ok(())
    }
}

/// Shared memory handle. Lock scope is explicit and writes reject invalid ranges.
#[derive(Debug, Clone)]
pub struct Memory(Arc<RwLock<Vec<u8>>>, Arc<AtomicBool>);
impl Memory {
    /// Reject remote memory writes while retaining application with_mut access.
    /// In-flight operations which already checked the flag may complete.
    pub fn set_read_only(&self, read_only: bool) {
        self.1.store(read_only, Ordering::Release);
    }

    /// Run an application update while excluding server reads/writes.
    /// Do not call back into this same memory handle while holding the guard.
    pub fn with_mut<T>(&self, update: impl FnOnce(&mut [u8]) -> T) -> Result<T> {
        let mut bytes = self
            .0
            .write()
            .map_err(|_| Error::Protocol("poisoned memory lock"))?;
        Ok(update(&mut bytes))
    }
    /// Copy the current contents under a shared read lock.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        Ok(self
            .0
            .read()
            .map_err(|_| Error::Protocol("poisoned memory lock"))?
            .clone())
    }
}

/// Shared registered areas and explicitly emulated CPU/system information.
struct Shared {
    memory: RwLock<BTreeMap<(u8, u16), Memory>>,
    szls: RwLock<BTreeMap<(u16, u16), Vec<u8>>>,
    cpu: Mutex<CpuState>,
    clock: Mutex<DateTime>,
    events: Mutex<VecDeque<Event>>,
    config: ServerConfig,
    clients: Mutex<BTreeMap<usize, TcpStream>>,
    active: AtomicUsize,
    stopping: AtomicBool,
}
impl Shared {
    /// Queue an event and invoke its sink with no emulator lock held.
    fn emit(&self, kind: EventKind, peer: SocketAddr, function: u8) {
        if !self.config.events.contains(&kind) {
            return;
        }
        let event = Event {
            kind,
            peer,
            function,
        };
        if let Ok(mut queue) = self.events.lock() {
            if queue.len() == self.config.event_capacity {
                queue.pop_front();
            }
            queue.push_back(event.clone());
        }
        if let Some(hook) = &self.config.hook {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(&event)));
        }
    }
    /// Clone an area handle without holding the registry while acquiring memory locks.
    fn memory(&self, area: Area, db: u16) -> Result<Memory> {
        self.memory
            .read()
            .map_err(|_| Error::Protocol("poisoned area registry"))?
            .get(&(area as u8, db))
            .cloned()
            .ok_or(Error::Item(0x0a))
    }
}

/// Running emulator. Dropping or stopping closes clients and joins all workers.
pub struct Server {
    endpoint: SocketAddr,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    /// Bind and start accepting sessions; register memory through the returned handle.
    pub fn start(config: ServerConfig) -> Result<Self> {
        config.validate()?;
        let listener = TcpListener::bind(config.bind)?;
        let endpoint = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            memory: Default::default(),
            szls: Default::default(),
            cpu: Mutex::new(CpuState::Run),
            clock: Mutex::new(DateTime::new(
                crate::codec::datetime::Date::new(2020, 1, 1)?,
                0,
                0,
                0,
                0,
            )?),
            events: Default::default(),
            clients: Default::default(),
            active: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
            config,
        });
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("snap7-accept".into())
            .spawn(move || {
                let mut workers: Vec<JoinHandle<()>> = Vec::new();
                let mut next_id = 0usize;
                while !state.stopping.load(Ordering::Acquire) {
                    // Reap completed sessions continuously instead of retaining unbounded handles.
                    let mut index = 0;
                    while index < workers.len() {
                        if workers[index].is_finished() {
                            let _ = workers.swap_remove(index).join();
                        } else {
                            index += 1;
                        }
                    }
                    match listener.accept() {
                        Ok((stream, peer)) => {
                            if state.active.load(Ordering::Acquire) >= state.config.max_clients {
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                            next_id = next_id.wrapping_add(1);
                            let id = next_id;
                            let Ok(clone) = stream.try_clone() else {
                                continue;
                            };
                            if let Ok(mut clients) = state.clients.lock() {
                                clients.insert(id, clone);
                            }
                            state.active.fetch_add(1, Ordering::AcqRel);
                            let client_state = state.clone();
                            match thread::Builder::new().name("snap7-session".into()).spawn(
                                move || {
                                    client_state.emit(EventKind::Connected, peer, 0);
                                    if engine::serve(stream, &client_state, peer).is_err()
                                        && !client_state.stopping.load(Ordering::Acquire)
                                    {
                                        client_state.emit(EventKind::Error, peer, 0);
                                    }
                                    if let Ok(mut clients) = client_state.clients.lock() {
                                        clients.remove(&id);
                                    }
                                    client_state.active.fetch_sub(1, Ordering::AcqRel);
                                    client_state.emit(EventKind::Disconnected, peer, 0);
                                },
                            ) {
                                Ok(worker) => workers.push(worker),
                                Err(_) => {
                                    if let Ok(mut clients) = state.clients.lock() {
                                        clients.remove(&id);
                                    }
                                    state.active.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => break,
                    }
                }
                // Check under the registry lock after the loop so stop/accept cannot miss a socket.
                if let Ok(clients) = state.clients.lock() {
                    for stream in clients.values() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
                for worker in workers {
                    let _ = worker.join();
                }
            })?;
        Ok(Self {
            endpoint,
            shared,
            worker: Some(worker),
        })
    }
    /// Actual bound endpoint.
    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint
    }
    /// Current admitted session count, including sessions negotiating COTP.
    pub fn client_count(&self) -> usize {
        self.shared.active.load(Ordering::Acquire)
    }
    /// Register a fixed-size area, returning a lockable shared handle.
    pub fn register_area(&self, area: Area, db: u16, bytes: Vec<u8>) -> Result<Memory> {
        if bytes.is_empty() || bytes.len() > 0x200000 || (area != Area::Db && db != 0) {
            return Err(Error::InvalidInput("invalid server area size or DB"));
        }
        let mut memory = self
            .shared
            .memory
            .write()
            .map_err(|_| Error::Protocol("poisoned area registry"))?;
        if memory.contains_key(&(area as u8, db)) {
            return Err(Error::InvalidInput("area already registered"));
        }
        let handle = Memory(
            Arc::new(RwLock::new(bytes)),
            Arc::new(AtomicBool::new(false)),
        );
        memory.insert((area as u8, db), handle.clone());
        Ok(handle)
    }
    /// Unregister an area; in-flight operations holding a handle may complete.
    pub fn unregister_area(&self, area: Area, db: u16) -> Result<()> {
        self.shared
            .memory
            .write()
            .map_err(|_| Error::Protocol("poisoned area registry"))?
            .remove(&(area as u8, db))
            .ok_or(Error::InvalidInput("area not registered"))?;
        Ok(())
    }
    /// Register a complete SZL body, including record-length/count header.
    pub fn register_szl(&self, id: u16, index: u16, body: Vec<u8>) -> Result<()> {
        if body.len() < 4 || body.len() > 1_000_000 {
            return Err(Error::InvalidInput("invalid SZL body size"));
        }
        let width = usize::from(u16::from_be_bytes([body[0], body[1]]));
        let count = usize::from(u16::from_be_bytes([body[2], body[3]]));
        if 4 + width * count != body.len() {
            return Err(Error::InvalidInput("SZL record count/length mismatch"));
        }
        self.shared
            .szls
            .write()
            .map_err(|_| Error::Protocol("poisoned SZL registry"))?
            .insert((id, index), body);
        Ok(())
    }
    /// Set emulated CPU state without running a PLC program.
    pub fn set_cpu_state(&self, state: CpuState) -> Result<()> {
        *self
            .shared
            .cpu
            .lock()
            .map_err(|_| Error::Protocol("poisoned CPU state"))? = state;
        Ok(())
    }
    /// Pop the oldest queued event.
    pub fn pick_event(&self) -> Option<Event> {
        self.shared.events.lock().ok()?.pop_front()
    }
    /// Clear queued events.
    pub fn clear_events(&self) {
        if let Ok(mut events) = self.shared.events.lock() {
            events.clear();
        }
    }
    /// Close sockets and wait for sessions to finish. Safe to call repeatedly.
    pub fn stop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        if let Ok(clients) = self.shared.clients.lock() {
            for stream in clients.values() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Server {
    /// Release the listener and active sessions deterministically.
    fn drop(&mut self) {
        self.stop();
    }
}
