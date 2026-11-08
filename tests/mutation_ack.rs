//! Service-level mutation acknowledgements must preserve definite versus uncertain outcomes.

mod common;
use common::*;
#[cfg(feature = "blocking")]
use rs_snap7::blocking::Client;
use rs_snap7::{
    ClientConfig, Error, Result,
    codec::datetime::{Date, DateTime},
    protocol::{Pdu, PduType},
};
use std::{
    io::Read,
    net::TcpListener,
    thread::{self, JoinHandle},
};

/// Public entry points sharing the same no-replay mutation acknowledgement rules.
#[derive(Debug, Clone, Copy)]
enum Mutation {
    Stop,
    HotStart,
    ColdStart,
    Compress,
    Copy,
    Clock,
    Password,
    Clear,
}
const MUTATIONS: [Mutation; 8] = [
    Mutation::Stop,
    Mutation::HotStart,
    Mutation::ColdStart,
    Mutation::Compress,
    Mutation::Copy,
    Mutation::Clock,
    Mutation::Password,
    Mutation::Clear,
];
/// Independently generated response faults or definite service refusals.
#[derive(Debug, Clone, Copy)]
enum Reply {
    WrongService,
    Payload,
    Continuation,
    Refusal,
}
/// Timestamp encodable by the PLC's DT clock format.
fn clock() -> DateTime {
    DateTime::new(Date::new(2024, 2, 29).unwrap(), 12, 0, 0, 0).unwrap()
}
impl Mutation {
    /// Execute a blocking operation without exposing the private session helper.
    #[cfg(feature = "blocking")]
    fn execute(self, client: &Client) -> Result<()> {
        match self {
            Self::Stop => client.plc_stop(),
            Self::HotStart => client.plc_hot_start(),
            Self::ColdStart => client.plc_cold_start(),
            Self::Compress => client.compress(),
            Self::Copy => client.copy_ram_to_rom(),
            Self::Clock => client.set_plc_datetime(clock()),
            Self::Password => client.set_session_password("SECRET"),
            Self::Clear => client.clear_session_password(),
        }
    }
    /// Execute the equivalent native asynchronous operation.
    async fn execute_async(self, client: &rs_snap7::Client) -> Result<()> {
        match self {
            Self::Stop => client.plc_stop().await,
            Self::HotStart => client.plc_hot_start().await,
            Self::ColdStart => client.plc_cold_start().await,
            Self::Compress => client.compress().await,
            Self::Copy => client.copy_ram_to_rom().await,
            Self::Clock => client.set_plc_datetime(clock()).await,
            Self::Password => client.set_session_password("SECRET").await,
            Self::Clear => client.clear_session_password().await,
        }
    }
    /// Stable mutation category independent of wire packet details.
    fn category(self) -> &'static str {
        match self {
            Self::Clock => "set_clock",
            Self::Password | Self::Clear => "session_password",
            _ => "plc_control",
        }
    }
}

/// Negotiate, receive exactly one mutation, then return a matching-reference faulty ACK.
fn peer(mutation: Mutation, reply: Reply) -> (ClientConfig, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    config.recovery.enabled = true;
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        let job = Pdu::decode(&read_frame(&mut stream)[3..]).unwrap();
        let mut response = if job.kind == PduType::Job {
            Pdu {
                kind: PduType::AckData,
                reference: job.reference,
                error: 0,
                parameters: vec![job.parameters[0]],
                data: vec![],
            }
        } else {
            Pdu {
                kind: PduType::UserData,
                reference: job.reference,
                error: 0,
                parameters: vec![
                    0,
                    1,
                    0x12,
                    8,
                    0x12,
                    0x80 | (job.parameters[5] & 15),
                    job.parameters[6],
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
                data: vec![0x0a, 0, 0, 0],
            }
        };
        match reply {
            Reply::WrongService => {
                if response.kind == PduType::UserData {
                    response.parameters[6] ^= 0x7f;
                } else {
                    response.parameters[0] ^= 1;
                }
            }
            Reply::Payload => {
                response.data = if response.kind == PduType::UserData {
                    vec![0xff, 9, 0, 1, 42]
                } else {
                    vec![42]
                };
            }
            Reply::Continuation => {
                response.parameters[9] = 1;
            }
            Reply::Refusal => {
                if response.kind == PduType::UserData {
                    response.parameters[10..12].copy_from_slice(&0xd602u16.to_be_bytes());
                } else {
                    response.error = 0x8702;
                }
            }
        }
        let mut frame = vec![2, 0xf0, 0x80];
        frame.extend(response.encode().unwrap());
        send_frame(&mut stream, &frame);
        if matches!(reply, Reply::Refusal) {
            assert_eq!(read_frame(&mut stream)[1], 0x80);
        } else {
            // Invalid acknowledgements must close the stream, never replay the mutation.
            let mut byte = [0];
            match stream.read(&mut byte) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("{mutation:?}/{reply:?}: expected closed socket, got {other:?}"),
            }
        }
    });
    (config, worker)
}
/// Assert that protocol failures retain their source and service category.
fn uncertain(result: Result<()>, mutation: Mutation) {
    match result {
        Err(Error::AmbiguousOperation { operation, source }) => {
            assert_eq!(operation, mutation.category());
            assert!(matches!(*source, Error::Protocol(_)));
        }
        other => panic!("{mutation:?}: expected ambiguous operation, got {other:?}"),
    }
}
/// Definite PLC/authentication refusals must remain directly inspectable.
fn refused(result: Result<()>, mutation: Mutation) {
    match mutation {
        Mutation::Password | Mutation::Clear => {
            assert!(matches!(result, Err(Error::Authentication(0xd602))))
        }
        Mutation::Clock => assert!(matches!(result, Err(Error::Plc(0xd602)))),
        _ => assert!(matches!(result, Err(Error::Plc(0x8702)))),
    }
}

/// All blocking control/clock/password APIs classify malformed service ACKs.
#[test]
#[cfg(feature = "blocking")]
fn malformed_mutation_acknowledgements_are_uncertain_and_never_replayed() {
    for mutation in MUTATIONS {
        for reply in [Reply::WrongService, Reply::Payload, Reply::Continuation] {
            if matches!(reply, Reply::Continuation) && mutation.category() == "plc_control" {
                continue;
            }
            let (config, worker) = peer(mutation, reply);
            let client = Client::connect(config).unwrap();
            uncertain(mutation.execute(&client), mutation);
            assert!(!client.is_connected().unwrap());
            worker.join().unwrap();
        }
    }
}
/// The same distinction holds through native asynchronous session execution.
#[tokio::test]
async fn async_malformed_mutation_acknowledgements_are_uncertain() {
    for mutation in MUTATIONS {
        for reply in [Reply::WrongService, Reply::Payload, Reply::Continuation] {
            if matches!(reply, Reply::Continuation) && mutation.category() == "plc_control" {
                continue;
            }
            let (config, worker) = peer(mutation, reply);
            let client = rs_snap7::Client::connect(config).await.unwrap();
            uncertain(mutation.execute_async(&client).await, mutation);
            assert!(!client.is_connected().await);
            worker.join().unwrap();
        }
    }
}
/// Do not turn explicit peer rejection into an ambiguous result.
#[test]
#[cfg(feature = "blocking")]
fn definite_mutation_refusals_preserve_codes_and_connection() {
    for mutation in MUTATIONS {
        let (config, worker) = peer(mutation, Reply::Refusal);
        let client = Client::connect(config).unwrap();
        refused(mutation.execute(&client), mutation);
        assert!(client.is_connected().unwrap());
        client.disconnect().unwrap();
        worker.join().unwrap();
    }
}
/// Async refusals preserve the same errors and leave the connection reusable.
#[tokio::test]
async fn async_definite_mutation_refusals_preserve_codes() {
    for mutation in MUTATIONS {
        let (config, worker) = peer(mutation, Reply::Refusal);
        let client = rs_snap7::Client::connect(config).await.unwrap();
        refused(mutation.execute_async(&client).await, mutation);
        assert!(client.is_connected().await);
        client.disconnect().await.unwrap();
        worker.join().unwrap();
    }
}

/// Exhaust the setup grant and verify no mutation bytes precede explicit disconnect.
fn limited_peer() -> (ClientConfig, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    config.rate_limit = rs_snap7::policy::RateLimit {
        max_per_second: 0.01,
        behavior: rs_snap7::policy::RateBehavior::Reject,
        ..Default::default()
    };
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        assert_eq!(read_frame(&mut stream)[1], 0x80);
    });
    (config, worker)
}

/// Limiter/manual-close errors occur before sending and remain definite.
#[test]
#[cfg(feature = "blocking")]
fn unsent_mutations_preserve_rate_limit_and_disconnected_errors() {
    let (config, worker) = limited_peer();
    let client = Client::connect(config).unwrap();
    for mutation in MUTATIONS {
        assert!(matches!(mutation.execute(&client), Err(Error::RateLimited)));
    }
    assert!(client.is_connected().unwrap());
    client.disconnect().unwrap();
    worker.join().unwrap();
    for mutation in MUTATIONS {
        assert!(matches!(
            mutation.execute(&client),
            Err(Error::Disconnected)
        ));
    }
}

/// Async pre-send rejection has identical classification and does not close the socket.
#[tokio::test]
async fn async_unsent_mutations_preserve_definite_errors() {
    let (config, worker) = limited_peer();
    let client = rs_snap7::Client::connect(config).await.unwrap();
    for mutation in MUTATIONS {
        assert!(matches!(
            mutation.execute_async(&client).await,
            Err(Error::RateLimited)
        ));
    }
    assert!(client.is_connected().await);
    client.disconnect().await.unwrap();
    worker.join().unwrap();
    for mutation in MUTATIONS {
        assert!(matches!(
            mutation.execute_async(&client).await,
            Err(Error::Disconnected)
        ));
    }
}
