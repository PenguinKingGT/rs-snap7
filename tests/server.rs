//! Client/server integration plus independent malformed requests and shutdown.
#![cfg(feature = "server")]
use rs_snap7::{
    Area, BlockType, Client, ClientConfig, CpuState, Error,
    server::{EventKind, Server, ServerConfig},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn server() -> Server {
    let mut config = ServerConfig::new("127.0.0.1:0".parse().unwrap());
    config.pdu_length = 128;
    config.timeout = Duration::from_millis(800);
    Server::start(config).unwrap()
}
fn client(server: &Server) -> Client {
    Client::connect(ClientConfig::new(server.local_addr(), 0, 1).unwrap()).unwrap()
}

#[test]
fn registered_memory_typed_batch_bits_control_and_metadata_work() {
    let mut server = server();
    let memory = server.register_area(Area::Db, 1, vec![0; 600]).unwrap();
    server
        .register_area(Area::Markers, 0, vec![0xaa; 128])
        .unwrap();
    let client = client(&server);
    client.db_write_int(1, 2, -123).unwrap();
    assert_eq!(client.db_read_int(1, 2).unwrap(), -123);
    let bytes: Vec<_> = (0..500).map(|i| i as u8).collect();
    client.db_write(1, 20, &bytes).unwrap();
    assert_eq!(client.db_read(1, 20, 500).unwrap(), bytes);
    assert_eq!(memory.snapshot().unwrap()[2..4], (-123i16).to_be_bytes());
    memory.with_mut(|bytes| bytes[10] = 0x42).unwrap();
    assert_eq!(client.db_read_byte(1, 10).unwrap(), 0x42);
    let addresses = [
        rs_snap7::Address::db(1, 2, 2).unwrap(),
        rs_snap7::Address::db(8, 0, 1).unwrap(),
    ];
    let result = client.read_multi_vars(&addresses).unwrap();
    assert!(result[0].is_ok());
    assert!(matches!(result[1], Err(Error::Item(0x0a))));
    client.plc_stop().unwrap();
    assert_eq!(client.get_cpu_state().unwrap(), CpuState::Stop);
    client.plc_hot_start().unwrap();
    assert_eq!(client.get_cpu_state().unwrap(), CpuState::Run);
    assert_eq!(client.list_blocks().unwrap().count(BlockType::Db), 1);
    assert_eq!(
        client.get_block_info(BlockType::Db, 1).unwrap().mc7_size,
        600
    );
    assert_eq!(
        client.list_block_entries(BlockType::Db).unwrap()[0].number,
        1
    );
    assert!(client.list_block_entries(BlockType::Fc).unwrap().is_empty());
    assert_eq!(client.upload(1).unwrap(), memory.snapshot().unwrap());
    let full = client.full_upload(BlockType::Db, 1).unwrap();
    client.download(&full, Some(1)).unwrap();
    client.disconnect().unwrap();
    server.stop();
}

#[test]
fn block_upload_download_delete_and_clock_round_trip() {
    use rs_snap7::codec::datetime::{Date, DateTime};
    let mut server = server();
    server.register_area(Area::Db, 3, vec![42; 236]).unwrap();
    let client = client(&server);
    let full = client.full_upload(BlockType::Db, 3).unwrap();
    client.delete(BlockType::Db, 3).unwrap();
    assert!(client.list_block_entries(BlockType::Db).unwrap().is_empty());
    client.download(&full, None).unwrap();
    assert_eq!(client.upload(3).unwrap(), vec![42; 236]);
    let time = DateTime::new(Date::new(2024, 2, 29).unwrap(), 12, 34, 56, 123_000_000).unwrap();
    client.set_plc_datetime(time).unwrap();
    assert_eq!(client.get_plc_datetime().unwrap(), time);
    client.set_session_password("secret").unwrap();
    client.clear_session_password().unwrap();
    client.disconnect().unwrap();
    server.stop();
}

#[test]
fn custom_szl_fragments_and_independent_sessions() {
    let mut server = server();
    server.register_area(Area::Db, 1, vec![0; 32]).unwrap();
    let mut body = vec![0, 10, 0, 30];
    body.extend(vec![0xa5; 300]);
    server.register_szl(0x1234, 2, body.clone()).unwrap();
    let first = client(&server);
    let second = client(&server);
    assert_eq!(first.read_szl(0x1234, 2).unwrap().data, body);
    first.db_write_int(1, 0, 22).unwrap();
    assert_eq!(second.db_read_int(1, 0).unwrap(), 22);
    assert!(server.client_count() >= 2);
    first.disconnect().unwrap();
    second.disconnect().unwrap();
    server.stop();
}

#[test]
fn event_hook_panics_are_isolated_and_queue_is_bounded() {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let mut config = ServerConfig::new("127.0.0.1:0".parse().unwrap());
    config.event_capacity = 2;
    config.hook = Some(Arc::new(move |event| {
        observed.fetch_add(1, Ordering::Relaxed);
        if event.kind == EventKind::Read {
            panic!("test hook");
        }
    }));
    let mut server = Server::start(config).unwrap();
    server.register_area(Area::Db, 1, vec![0; 16]).unwrap();
    let client = client(&server);
    for _ in 0..3 {
        client.db_read(1, 0, 1).unwrap();
    }
    assert!(count.load(Ordering::Relaxed) >= 4);
    assert!(server.pick_event().is_some());
    assert!(server.pick_event().is_some());
    assert!(server.pick_event().is_none());
    client.disconnect().unwrap();
    server.stop();
}

#[test]
fn invalid_frames_and_idle_clients_cannot_block_shutdown() {
    use std::{io::Write, net::TcpStream, time::Instant};
    let mut server = server();
    let mut malformed = TcpStream::connect(server.local_addr()).unwrap();
    malformed.write_all(&[3, 0, 0, 5, 0]).unwrap();
    let idle = TcpStream::connect(server.local_addr()).unwrap();
    let before = Instant::now();
    server.stop();
    assert!(before.elapsed() < Duration::from_secs(2));
    drop(idle);
    let mut config = ServerConfig::new("127.0.0.1:0".parse().unwrap());
    config.max_clients = 0;
    assert!(Server::start(config).is_err());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn native_async_client_uses_the_same_emulator_services() {
    let mut server = server();
    server.register_area(Area::Db, 1, vec![0; 64]).unwrap();
    let client =
        rs_snap7::AsyncClient::connect(ClientConfig::new(server.local_addr(), 0, 1).unwrap())
            .await
            .unwrap();
    client.db_write_int(1, 0, -42).await.unwrap();
    assert_eq!(client.db_read_int(1, 0).await.unwrap(), -42);
    assert_eq!(client.get_cpu_state().await.unwrap(), CpuState::Run);
    assert_eq!(client.upload(1).await.unwrap().len(), 64);
    client.disconnect().await.unwrap();
    server.stop();
}
