//! Bounded sustained multi-session traffic and connection-resource cleanup.
#![cfg(feature = "server")]
use rs_snap7::{
    Area, Client, ClientConfig,
    server::{Server, ServerConfig},
};
use std::time::{Duration, Instant};

#[tokio::test]
async fn sustained_independent_sessions_preserve_memory_and_release_resources() {
    let mut config = ServerConfig::new("127.0.0.1:0".parse().unwrap());
    config.pdu_length = 128;
    config.event_capacity = 32;
    let mut server = Server::start(config).unwrap();
    for db in 1..=8 {
        server.register_area(Area::Db, db, vec![0; 4096]).unwrap();
    }
    let endpoint = server.local_addr();
    let workers: Vec<_> = (1..=8)
        .map(|db| {
            tokio::spawn(async move {
                let client = Client::connect(ClientConfig::new(endpoint, 0, 1).unwrap())
                    .await
                    .unwrap();
                for iteration in 0..200 {
                    let offset = (iteration % 64) * 32;
                    let bytes = vec![(iteration + u32::from(db)) as u8; 32];
                    client.db_write(db, offset, &bytes).await.unwrap();
                    assert_eq!(client.db_read(db, offset, 32).await.unwrap(), bytes);
                }
                client.disconnect().await.unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.await.unwrap();
    }
    for _ in 0..64 {
        let client = Client::connect(ClientConfig::new(endpoint, 0, 1).unwrap())
            .await
            .unwrap();
        client.disconnect().await.unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.client_count() != 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(server.client_count(), 0);
    let before = Instant::now();
    server.stop();
    assert!(before.elapsed() < Duration::from_secs(2));
}
