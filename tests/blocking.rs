//! Blocking adapters share the async core while preserving runtime and callback invariants.
#![cfg(feature = "blocking")]

mod common;
use common::*;
use rs_snap7::{
    ClientConfig, Error,
    blocking::Client,
    protocol::{Pdu, PduType},
};
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Context rejection happens before a TCP accept, even when config is otherwise valid.
#[tokio::test(flavor = "current_thread")]
async fn entered_runtime_rejects_blocking_connect_before_network_io() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(
        Client::connect(ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap()),
        Err(Error::BlockingInAsyncContext)
    ));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

/// A caller waiting synchronously must not freeze the independent heartbeat worker.
#[test]
fn idle_heartbeat_advances_and_last_clone_drop_closes_the_socket() {
    use std::io::Read;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    config.recovery.heartbeat_interval = Some(Duration::from_millis(10));
    let (send, receive) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        for _ in 0..2 {
            let job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
            assert_eq!(&job.data[4..], &[4, 0x24, 0, 0]);
            let response = Pdu {
                kind: PduType::UserData,
                reference: job.reference,
                error: 0,
                parameters: vec![0, 1, 0x12, 8, 0x12, 0x84, 1, 0, 0, 0, 0, 0],
                data: vec![0xff, 9, 0, 12, 4, 0x24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8],
            };
            let mut raw = vec![2, 0xf0, 0x80];
            raw.extend(response.encode().unwrap());
            send_frame(&mut stream, &raw);
        }
        send.send(()).unwrap();
        let mut scratch = [0; 1024];
        loop {
            match stream.read(&mut scratch) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    break;
                }
                error => panic!("final client drop did not close heartbeat socket: {error:?}"),
            }
        }
    });
    let client = Client::connect(config).unwrap();
    let clone = client.clone();
    drop(client);
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(clone.is_connected().unwrap());
    drop(clone);
    worker.join().unwrap();
}

#[cfg(feature = "server")]
mod server_scenarios {
    //! More complex lifetime and hook cases use the tested emulator.
    use super::*;
    use rs_snap7::{
        Area,
        observability::OperationHook,
        server::{Server, ServerConfig},
    };

    /// Create bounded local memory without a PLC.
    fn server() -> Server {
        let server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap())).unwrap();
        server.register_area(Area::Db, 1, vec![0; 128]).unwrap();
        server
    }

    /// Runtime context errors leave an established session untouched; Drop is always safe.
    #[tokio::test(flavor = "current_thread")]
    async fn existing_blocking_client_rejects_nested_calls_and_can_drop_inside_tokio() {
        let server = server();
        let endpoint = server.local_addr();
        let client = thread::spawn(move || {
            Client::connect(ClientConfig::new(endpoint, 0, 1).unwrap()).unwrap()
        })
        .join()
        .unwrap();
        assert!(matches!(
            client.db_write_int(1, 0, 42),
            Err(Error::BlockingInAsyncContext)
        ));
        assert!(matches!(
            client.disconnect(),
            Err(Error::BlockingInAsyncContext)
        ));
        let clone = client.clone();
        thread::spawn(move || {
            assert!(clone.is_connected().unwrap());
            assert_eq!(clone.db_read_int(1, 0).unwrap(), 0);
        })
        .join()
        .unwrap();
        drop(client);
    }

    /// Deferred callbacks run on the original caller after the runtime context/locks exit.
    #[test]
    fn hook_reentry_and_single_logical_event_preserve_core_statistics() {
        let server = server();
        let owner = thread::current().id();
        let holder: Arc<Mutex<Option<Client>>> = Arc::new(Mutex::new(None));
        let current = holder.clone();
        let mut config = ClientConfig::new(server.local_addr(), 0, 1).unwrap();
        config.operation_hook = OperationHook::new(move |event| {
            assert_eq!(thread::current().id(), owner);
            if event.operation == "db_read_int" {
                let client = current.lock().unwrap();
                assert_eq!(client.as_ref().unwrap().pdu_length().unwrap(), 480);
                assert_eq!(
                    client
                        .as_ref()
                        .unwrap()
                        .operation_stats()
                        .last
                        .unwrap()
                        .operation,
                    "db_read_int"
                );
            }
        });
        let client = Client::connect(config).unwrap();
        *holder.lock().unwrap() = Some(client.clone());
        let before = client.operation_stats().completed;
        assert_eq!(client.db_read_int(1, 0).unwrap(), 0);
        assert_eq!(client.operation_stats().completed, before + 1);
        assert_eq!(client.operation_stats().hook_panics, 0);
        client.disconnect().unwrap();
        *holder.lock().unwrap() = None;
    }

    /// Generic scalar callers can retain non-Send/non-static local codec types.
    #[test]
    fn scalar_delegation_does_not_require_send_or_static_results() {
        use rs_snap7::{Result, codec::Scalar};
        use std::rc::Rc;
        struct Local(Rc<u16>);
        impl Scalar for Local {
            const SIZE: usize = 2;
            fn decode(bytes: &[u8]) -> Result<Self> {
                Ok(Self(Rc::new(u16::decode(bytes)?)))
            }
            fn encode(self, bytes: &mut [u8]) -> Result<()> {
                (*self.0).encode(bytes)
            }
        }
        let server = server();
        let client =
            Client::connect(ClientConfig::new(server.local_addr(), 0, 1).unwrap()).unwrap();
        client.db_write_value(1, 0, Local(Rc::new(42))).unwrap();
        assert_eq!(*client.db_read_value::<Local>(1, 0).unwrap().0, 42);
        client.disconnect().unwrap();
    }

    /// Independent clients and their shared runtime still execute concurrent I/O.
    #[test]
    fn blocking_pool_delegates_to_core_and_rejects_shared_sessions() {
        use rs_snap7::{Address, blocking::ClientPool};
        let server = server();
        let first = Client::connect(ClientConfig::new(server.local_addr(), 0, 1).unwrap()).unwrap();
        let second =
            Client::connect(ClientConfig::new(server.local_addr(), 0, 1).unwrap()).unwrap();
        assert!(ClientPool::new(vec![first.clone(), first.clone()]).is_err());
        let pool = ClientPool::new(vec![first, second]).unwrap();
        let result = pool
            .read_multi_vars(&[Address::db(1, 0, 1).unwrap(), Address::db(1, 4, 1).unwrap()])
            .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(Result::is_ok));
    }
}
