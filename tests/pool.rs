//! Independent peers require concurrent dispatch before either can reply.

mod common;
use common::*;
#[cfg(feature = "blocking")]
use rs_snap7::blocking::{Client, ClientPool};
use rs_snap7::{Address, ClientConfig};
use std::{
    net::TcpListener,
    sync::{Arc, Condvar, Mutex},
    thread,
};

/// Two peers wait for both read requests, proving independent sessions run in parallel.
#[test]
#[cfg(feature = "blocking")]
fn pool_dispatches_concurrently_and_restores_input_order() {
    let barrier = Arc::new((Mutex::new(0), Condvar::new()));
    let mut workers = Vec::new();
    let mut clients = Vec::new();
    for value in [11, 22] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 480);
            let request = read_frame(&mut stream);
            meet(&barrier);
            let p = &request[3..];
            let reply = [
                2, 0xf0, 0x80, 0x32, 3, 0, 0, p[4], p[5], 0, 2, 0, 5, 0, 0, 4, 1, 0xff, 4, 0, 8,
                value,
            ];
            send_frame(&mut stream, &reply);
            read_frame(&mut stream);
        }));
        clients.push(Client::connect(config).unwrap());
    }
    assert!(ClientPool::new(vec![clients[0].clone(), clients[0].clone()]).is_err());
    let pool = ClientPool::new(clients.clone()).unwrap();
    let results = pool
        .read_multi_vars(&[Address::db(1, 0, 1).unwrap(); 2])
        .unwrap();
    assert_eq!(results[0].as_ref().unwrap(), &[11]);
    assert_eq!(results[1].as_ref().unwrap(), &[22]);
    for client in clients {
        client.disconnect().unwrap();
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(ClientPool::new(vec![]).is_err());
}

/// The native async pool also requires concurrent requests on independent connections.
#[tokio::test(flavor = "current_thread")]
async fn async_pool_dispatches_independent_sessions() {
    use rs_snap7::{Client as AsyncClient, ClientPool as AsyncClientPool};
    let barrier = Arc::new((Mutex::new(0), Condvar::new()));
    let mut workers = Vec::new();
    let mut clients = Vec::new();
    for value in [33, 44] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 480);
            let request = read_frame(&mut stream);
            meet(&barrier);
            let p = &request[3..];
            send_frame(
                &mut stream,
                &[
                    2, 0xf0, 0x80, 0x32, 3, 0, 0, p[4], p[5], 0, 2, 0, 5, 0, 0, 4, 1, 0xff, 4, 0,
                    8, value,
                ],
            );
            read_frame(&mut stream);
        }));
        clients.push(AsyncClient::connect(config).await.unwrap());
    }
    assert!(AsyncClientPool::new(vec![clients[0].clone(), clients[0].clone()]).is_err());
    let pool = AsyncClientPool::new(clients.clone()).unwrap();
    let results = pool
        .read_multi_vars(&[Address::db(1, 0, 1).unwrap(); 2])
        .await
        .unwrap();
    assert_eq!(results[0].as_ref().unwrap(), &[33]);
    assert_eq!(results[1].as_ref().unwrap(), &[44]);
    for client in clients {
        client.disconnect().await.unwrap();
    }
    for worker in workers {
        worker.join().unwrap();
    }
}

/// Require both peers within a bounded interval so regressions cannot hang CI.
fn meet(gate: &(Mutex<usize>, Condvar)) {
    let (count, changed) = gate;
    let mut count = count.lock().unwrap();
    *count += 1;
    changed.notify_all();
    let (count, _) = changed
        .wait_timeout_while(count, std::time::Duration::from_secs(2), |count| *count < 2)
        .unwrap();
    assert_eq!(*count, 2, "requests were not dispatched concurrently");
}
