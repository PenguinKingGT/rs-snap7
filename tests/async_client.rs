//! Async-only cancellation and concurrent session ownership regressions.

#![cfg(feature = "async")]

mod common;
use common::*;
use rs_snap7::{AsyncClient, ClientConfig, Error};
use std::{io::Read, net::TcpListener, thread, time::Duration};

/// Cancelling a transmitted job closes its socket instead of leaving a stale reply.
#[tokio::test(flavor = "current_thread")]
async fn cancelling_in_flight_job_invalidates_session() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    let (started, observed) = tokio::sync::oneshot::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        let request = read_frame(&mut stream);
        assert_eq!(request[13], 4);
        started.send(()).unwrap();
        let mut byte = [0];
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let client = AsyncClient::connect(config).await.unwrap();
    let other = client.clone();
    let operation = tokio::spawn(async move { other.db_read(1, 0, 1).await });
    observed.await.unwrap();
    operation.abort();
    assert!(operation.await.unwrap_err().is_cancelled());
    assert!(!client.is_connected().await);
    assert!(matches!(
        client.db_read(1, 0, 1).await,
        Err(Error::Disconnected)
    ));
    client.disconnect().await.unwrap();
    worker.join().unwrap();
}

/// A pending operation yields executor time and a lost response reaches its deadline.
#[tokio::test(flavor = "current_thread")]
async fn silent_peer_yields_and_times_out_without_blocking_runtime() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
    config.transport.timeout = Duration::from_millis(100);
    let (release, wait) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        negotiate(&mut stream, 480);
        read_frame(&mut stream);
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let client = AsyncClient::connect(config).await.unwrap();
    let (read, timer) = tokio::join!(client.db_read(1, 0, 1), async {
        tokio::task::yield_now().await;
        42
    });
    assert_eq!(timer, 42);
    assert!(matches!(read, Err(Error::Timeout)));
    assert!(!client.is_connected().await);
    release.send(()).unwrap();
    worker.join().unwrap();
}
