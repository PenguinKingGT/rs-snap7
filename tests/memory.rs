//! Stateful independent wire peer for raw and typed memory compatibility tests.

mod common;
use common::*;
use rs_snap7::{Client, ClientConfig, Error};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

/// A local wire peer with controlled failures and a bounded memory region.
struct Peer {
    endpoint: SocketAddr,
    memory: Arc<Mutex<Vec<u8>>>,
    worker: JoinHandle<()>,
}

impl Peer {
    /// Start an ephemeral peer; selected write exchanges may be rejected or lose their ACK.
    fn new(fail_at: Option<usize>, drop_at: Option<usize>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let memory = Arc::new(Mutex::new(vec![0; 1024]));
        let bytes = Arc::clone(&memory);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 128);
            let mut writes = 0;
            loop {
                let request = read_frame(&mut stream);
                if request[1] == 0x80 {
                    break;
                }
                assert_eq!(&request[..3], &[2, 0xf0, 0x80]);
                let p = &request[3..];
                let param_len = usize::from(u16::from_be_bytes([p[6], p[7]]));
                let params = &p[10..10 + param_len];
                let payload = &p[10 + param_len..];
                let mut data = Vec::new();
                let (kind, response_params) = if p[1] == 7 {
                    assert_eq!(&params[..7], &[0, 1, 0x12, 4, 0x11, 0x43, 3]);
                    let db: u16 = std::str::from_utf8(&payload[6..11])
                        .unwrap()
                        .parse()
                        .unwrap();
                    let mut info = vec![0; 78];
                    info[1] = 0x41;
                    info[12..14].copy_from_slice(&db.to_be_bytes());
                    info[40..42].copy_from_slice(&1024u16.to_be_bytes());
                    data.extend_from_slice(&[0xff, 9, 0, 78]);
                    data.extend(info);
                    (7, vec![0, 1, 0x12, 8, 0x12, 0x83, 3, 0, 0, 0, 0, 0])
                } else {
                    let item_count = usize::from(params[1]);
                    assert!(item_count <= 20);
                    if params[0] == 5 {
                        writes += 1;
                        if drop_at == Some(writes) {
                            return;
                        }
                    }
                    let mut payload_offset = 0;
                    for (index, address) in params[2..].chunks_exact(12).enumerate() {
                        let count = usize::from(u16::from_be_bytes([address[4], address[5]]));
                        let indexed = matches!(address[8], 0x1c | 0x1d);
                        let wire_offset =
                            u32::from_be_bytes([0, address[9], address[10], address[11]]) as usize;
                        let start = if indexed {
                            wire_offset * 2
                        } else {
                            wire_offset / 8
                        };
                        let size = count
                            * match address[3] {
                                4 | 5 | 0x1c | 0x1d => 2,
                                6..=8 => 4,
                                _ => 1,
                            };
                        let mut memory = bytes.lock().unwrap();
                        let valid = start + size <= memory.len();
                        if params[0] == 4 {
                            if valid {
                                data.extend_from_slice(&[0xff, if indexed { 9 } else { 4 }]);
                                data.extend_from_slice(
                                    &((if indexed { size } else { size * 8 }) as u16).to_be_bytes(),
                                );
                                data.extend_from_slice(&memory[start..start + size]);
                                if index + 1 < item_count && size % 2 == 1 {
                                    data.push(0);
                                }
                            } else {
                                data.extend_from_slice(&[5, 0, 0, 0]);
                            }
                        } else {
                            assert_eq!(params[0], 5);
                            if fail_at == Some(writes) || !valid {
                                data.push(5);
                            } else {
                                memory[start..start + size].copy_from_slice(
                                    &payload[payload_offset + 4..payload_offset + 4 + size],
                                );
                                data.push(0xff);
                            }
                            payload_offset += 4 + size;
                            if index + 1 < item_count && size % 2 == 1 {
                                payload_offset += 1;
                            }
                        }
                    }
                    (3, vec![params[0], params[1]])
                };
                let mut response = vec![2, 0xf0, 0x80, 0x32, kind, 0, 0, p[4], p[5]];
                response.extend_from_slice(&(response_params.len() as u16).to_be_bytes());
                response.extend_from_slice(&(data.len() as u16).to_be_bytes());
                if kind == 3 {
                    response.extend_from_slice(&[0, 0]);
                }
                response.extend(response_params);
                response.extend(data);
                send_frame(&mut stream, &response);
            }
        });
        Self {
            endpoint,
            memory,
            worker,
        }
    }

    /// Connect a real library client to this independently implemented peer.
    fn client(&self) -> Client {
        Client::connect(ClientConfig::new(self.endpoint, 0, 1).unwrap()).unwrap()
    }
}

/// Multi-PDU transfers, DB metadata discovery, get/fill, and empty operations.
#[test]
fn chunked_db_read_write_get_and_fill() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    let bytes: Vec<u8> = (0..600).map(|i| i as u8).collect();
    client.db_write(1, 10, &bytes).unwrap();
    assert_eq!(client.db_read(1, 10, 600).unwrap(), bytes);
    assert_eq!(&peer.memory.lock().unwrap()[10..610], bytes);
    assert_eq!(client.db_get(1, None).unwrap().len(), 1024);
    client.db_fill(1, 0xab, Some(600)).unwrap();
    assert_eq!(client.db_get(1, Some(600)).unwrap(), vec![0xab; 600]);
    client.db_fill(1, 0x55, None).unwrap();
    assert_eq!(client.db_get(1, None).unwrap(), vec![0x55; 1024]);
    assert_eq!(client.db_read(1, 0, 0).unwrap(), Vec::<u8>::new());
    client.db_write(1, 0, &[]).unwrap();
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Timer/counter offsets count 16-bit elements while normal areas count bytes.
#[test]
fn area_helpers_and_timer_counter_units() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    client.ct_write(2, &[1, 2, 3, 4]).unwrap();
    assert_eq!(client.ct_read(2, 2).unwrap(), [1, 2, 3, 4]);
    client.tm_write(4, &[5, 6]).unwrap();
    assert_eq!(client.tm_read(4, 1).unwrap(), [5, 6]);
    client.mb_write(0, &[7, 8]).unwrap();
    assert_eq!(client.mb_read(0, 2).unwrap(), [7, 8]);
    client.ab_write(0, &[9]).unwrap();
    assert_eq!(client.ab_read(0, 1).unwrap(), [9]);
    client.eb_write(0, &[10]).unwrap();
    assert_eq!(client.eb_read(0, 1).unwrap(), [10]);
    assert!(client.ct_write(0, &[1]).is_err());
    assert!(client.db_read(1, u32::MAX, 1).is_err());
    assert!(client.db_read(1, 0, usize::MAX).is_err());
    assert!(client.db_write(1, 0x1fffff, &[0, 0]).is_err());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// A definite later rejection reports acknowledged progress without claiming uncertainty.
#[test]
fn partial_write_reports_acknowledged_bytes() {
    let peer = Peer::new(Some(2), None);
    let client = peer.client();
    let error = client.db_write(1, 0, &[7; 300]).unwrap_err();
    assert!(matches!(error, Error::PartialWrite { confirmed: 100, .. }));
    assert_eq!(
        &peer.memory.lock().unwrap()[..101],
        &[vec![7; 100], vec![0]].concat()
    );
    assert!(client.is_connected());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// A lost write acknowledgement is never silently replayed and closes the session.
#[test]
fn lost_write_ack_reports_uncertain_range() {
    let peer = Peer::new(None, Some(2));
    let client = peer.client();
    let error = client.db_write(1, 0, &[7; 300]).unwrap_err();
    assert!(matches!(
        error,
        Error::AmbiguousWrite {
            confirmed: 100,
            uncertain: 100,
            ..
        }
    ));
    assert!(!client.is_connected());
    peer.worker.join().unwrap();
}

/// Typed arrays cross wire chunks while preserving signed/float representations.
#[test]
fn typed_scalars_arrays_and_strings_roundtrip() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    client.db_write_int(1, 0, i16::MIN).unwrap();
    assert_eq!(client.db_read_int(1, 0).unwrap(), i16::MIN);
    client.db_write_lreal(1, 2, -123.25).unwrap();
    assert_eq!(client.db_read_lreal(1, 2).unwrap(), -123.25);
    let array: Vec<i32> = (-60..60).collect();
    client.db_write_array(1, 10, &array).unwrap();
    assert_eq!(client.db_read_array::<i32>(1, 10, 120).unwrap(), array);
    assert!(client.db_read_array::<u64>(1, 0, usize::MAX).is_err());
    client.db_write_string(1, 500, "hé", 10).unwrap();
    assert_eq!(client.db_read_string(1, 500).unwrap(), "hé");
    client.db_write_wstring(1, 520, "中文", 10).unwrap();
    assert_eq!(client.db_read_wstring(1, 520).unwrap(), "中文");
    assert!(client.db_write_string(1, 0, "long", 1).is_err());
    assert!(client.db_write_wstring(1, 0, "😀", 10).is_err());
    client.db_write(1, 550, &[1, 2]).unwrap();
    assert!(client.db_read_string(1, 550).is_err());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Concurrent bit updates through clones hold one lock across read/modify/write.
#[test]
fn concurrent_bit_updates_preserve_other_bits() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    let workers: Vec<_> = (0..8)
        .map(|bit| {
            let other = client.clone();
            thread::spawn(move || other.db_write_bool(1, 0, bit, true).unwrap())
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(client.db_read_byte(1, 0).unwrap(), 255);
    for bit in 0..8 {
        assert!(client.db_read_bool(1, 0, bit).unwrap());
    }
    client.db_write_bool(1, 0, 3, false).unwrap();
    assert_eq!(client.db_read_byte(1, 0).unwrap(), 247);
    assert!(client.db_write_bool(1, 0, 8, true).is_err());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Mixed item results, odd padding, large-variable splitting, and original order.
#[test]
fn batch_reads_and_writes_preserve_item_results() {
    use rs_snap7::Address;
    let peer = Peer::new(None, None);
    let client = peer.client();
    let first = Address::db(1, 10, 301).unwrap();
    let second = Address::db(1, 500, 3).unwrap();
    let invalid = Address::db(1, 2000, 1).unwrap();
    let writes = client
        .write_multi_vars(&[(first, &[0x33; 301]), (invalid, &[7]), (second, &[1, 2, 3])])
        .unwrap();
    assert!(writes[0].is_ok());
    assert!(matches!(writes[1], Err(Error::Item(5))));
    assert!(writes[2].is_ok());
    let reads = client.read_multi_vars(&[second, invalid, first]).unwrap();
    assert_eq!(reads[0].as_ref().unwrap(), &[1, 2, 3]);
    assert!(matches!(reads[1], Err(Error::Item(5))));
    assert_eq!(reads[2].as_ref().unwrap(), &vec![0x33; 301]);
    assert!(client.read_multi_vars(&[second; 21]).is_err());
    assert!(client.write_multi_vars(&[(second, &[0])]).is_err());
    assert!(client.read_multi_vars(&[]).unwrap().is_empty());
    assert!(client.write_multi_vars(&[]).unwrap().is_empty());
    let reads = client.read_multi_vars(&[second; 20]).unwrap();
    assert!(reads.into_iter().all(|r| r.unwrap() == [1, 2, 3]));
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// I/O forcing uses process-image read/modify/write and preserves adjacent bits.
#[test]
fn force_and_cancel_preserve_neighbors() {
    use rs_snap7::Area;
    let peer = Peer::new(None, None);
    let client = peer.client();
    client.eb_write(0, &[0xaa]).unwrap();
    client.force_bit(Area::Inputs, 0, 0, true).unwrap();
    assert_eq!(client.eb_read(0, 1).unwrap(), [0xab]);
    client.cancel_force(Area::Inputs, 0, 1).unwrap();
    assert_eq!(client.eb_read(0, 1).unwrap(), [0xa9]);
    assert!(client.force_bit(Area::Db, 0, 0, true).is_err());
    assert!(client.force_bit(Area::Outputs, 0, 8, true).is_err());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// LOGO VM access reuses the same real S7 session and signed numeric semantics.
#[test]
fn logo_signed_values_and_preserved_vm_bits() {
    let peer = Peer::new(None, None);
    let client = peer.client();
    let logo = rs_snap7::Logo::from_client(client.clone());
    for (address, value) in [("V0", 255), ("VW10", -32768), ("VD20", -2147483648)] {
        logo.write(address, value).unwrap();
        assert_eq!(logo.read(address).unwrap(), value);
    }
    logo.write("V0.3", 0).unwrap();
    assert_eq!(logo.read("V0").unwrap(), 247);
    logo.write("V0.3", 1).unwrap();
    assert_eq!(logo.read("V0.3").unwrap(), 1);
    assert!(logo.write("VW10", 32768).is_err());
    assert!(logo.write("V0", -1).is_err());
    assert!(logo.read("V0junk").is_err());
    assert!(logo.client().is_connected());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_chunked_db_read_write_get_and_fill() {
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let bytes: Vec<u8> = (0..600).map(|i| i as u8).collect();
    client.db_write(1, 10, &bytes).await.unwrap();
    assert_eq!(client.db_read(1, 10, 600).await.unwrap(), bytes);
    assert_eq!(&peer.memory.lock().unwrap()[10..610], bytes);
    assert_eq!(client.db_get(1, None).await.unwrap().len(), 1024);
    client.db_fill(1, 0xab, Some(600)).await.unwrap();
    assert_eq!(client.db_get(1, Some(600)).await.unwrap(), vec![0xab; 600]);
    client.db_fill(1, 0x55, None).await.unwrap();
    assert_eq!(client.db_get(1, None).await.unwrap(), vec![0x55; 1024]);
    assert_eq!(client.db_read(1, 0, 0).await.unwrap(), Vec::<u8>::new());
    client.db_write(1, 0, &[]).await.unwrap();
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_area_helpers_and_timer_counter_units() {
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    client.ct_write(2, &[1, 2, 3, 4]).await.unwrap();
    assert_eq!(client.ct_read(2, 2).await.unwrap(), [1, 2, 3, 4]);
    client.tm_write(4, &[5, 6]).await.unwrap();
    assert_eq!(client.tm_read(4, 1).await.unwrap(), [5, 6]);
    client.mb_write(0, &[7, 8]).await.unwrap();
    assert_eq!(client.mb_read(0, 2).await.unwrap(), [7, 8]);
    client.ab_write(0, &[9]).await.unwrap();
    assert_eq!(client.ab_read(0, 1).await.unwrap(), [9]);
    client.eb_write(0, &[10]).await.unwrap();
    assert_eq!(client.eb_read(0, 1).await.unwrap(), [10]);
    assert!(client.ct_write(0, &[1]).await.is_err());
    assert!(client.db_read(1, u32::MAX, 1).await.is_err());
    assert!(client.db_read(1, 0, usize::MAX).await.is_err());
    assert!(client.db_write(1, 0x1fffff, &[0, 0]).await.is_err());
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_partial_write_reports_acknowledged_bytes() {
    let peer = Peer::new(Some(2), None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let error = client.db_write(1, 0, &[7; 300]).await.unwrap_err();
    assert!(matches!(error, Error::PartialWrite { confirmed: 100, .. }));
    assert_eq!(
        &peer.memory.lock().unwrap()[..101],
        &[vec![7; 100], vec![0]].concat()
    );
    assert!(client.is_connected().await);
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_lost_write_ack_reports_uncertain_range() {
    let peer = Peer::new(None, Some(2));
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let error = client.db_write(1, 0, &[7; 300]).await.unwrap_err();
    assert!(matches!(
        error,
        Error::AmbiguousWrite {
            confirmed: 100,
            uncertain: 100,
            ..
        }
    ));
    assert!(!client.is_connected().await);
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_typed_scalars_arrays_and_strings_roundtrip() {
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    client.db_write_int(1, 0, i16::MIN).await.unwrap();
    assert_eq!(client.db_read_int(1, 0).await.unwrap(), i16::MIN);
    client.db_write_lreal(1, 2, -123.25).await.unwrap();
    assert_eq!(client.db_read_lreal(1, 2).await.unwrap(), -123.25);
    let array: Vec<i32> = (-60..60).collect();
    client.db_write_array(1, 10, &array).await.unwrap();
    assert_eq!(
        client.db_read_array::<i32>(1, 10, 120).await.unwrap(),
        array
    );
    assert!(client.db_read_array::<u64>(1, 0, usize::MAX).await.is_err());
    client.db_write_string(1, 500, "hé", 10).await.unwrap();
    assert_eq!(client.db_read_string(1, 500).await.unwrap(), "hé");
    client.db_write_wstring(1, 520, "中文", 10).await.unwrap();
    assert_eq!(client.db_read_wstring(1, 520).await.unwrap(), "中文");
    assert!(client.db_write_string(1, 0, "long", 1).await.is_err());
    assert!(client.db_write_wstring(1, 0, "😀", 10).await.is_err());
    client.db_write(1, 550, &[1, 2]).await.unwrap();
    assert!(client.db_read_string(1, 550).await.is_err());
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_batch_reads_and_writes_preserve_item_results() {
    use rs_snap7::Address;
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let first = Address::db(1, 10, 301).unwrap();
    let second = Address::db(1, 500, 3).unwrap();
    let invalid = Address::db(1, 2000, 1).unwrap();
    let writes = client
        .write_multi_vars(&[(first, &[0x33; 301]), (invalid, &[7]), (second, &[1, 2, 3])])
        .await
        .unwrap();
    assert!(writes[0].is_ok());
    assert!(matches!(writes[1], Err(Error::Item(5))));
    assert!(writes[2].is_ok());
    let reads = client
        .read_multi_vars(&[second, invalid, first])
        .await
        .unwrap();
    assert_eq!(reads[0].as_ref().unwrap(), &[1, 2, 3]);
    assert!(matches!(reads[1], Err(Error::Item(5))));
    assert_eq!(reads[2].as_ref().unwrap(), &vec![0x33; 301]);
    assert!(client.read_multi_vars(&[second; 21]).await.is_err());
    assert!(client.write_multi_vars(&[(second, &[0])]).await.is_err());
    assert!(client.read_multi_vars(&[]).await.unwrap().is_empty());
    assert!(client.write_multi_vars(&[]).await.unwrap().is_empty());
    let reads = client.read_multi_vars(&[second; 20]).await.unwrap();
    assert!(reads.into_iter().all(|r| r.unwrap() == [1, 2, 3]));
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Native async counterpart of the blocking compatibility scenario.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_force_and_cancel_preserve_neighbors() {
    use rs_snap7::Area;
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    client.eb_write(0, &[0xaa]).await.unwrap();
    client.force_bit(Area::Inputs, 0, 0, true).await.unwrap();
    assert_eq!(client.eb_read(0, 1).await.unwrap(), [0xab]);
    client.cancel_force(Area::Inputs, 0, 1).await.unwrap();
    assert_eq!(client.eb_read(0, 1).await.unwrap(), [0xa9]);
    assert!(client.force_bit(Area::Db, 0, 0, true).await.is_err());
    assert!(client.force_bit(Area::Outputs, 0, 8, true).await.is_err());
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Async clones serialize bit updates while yielding during network waits.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_concurrent_bit_updates_preserve_neighbors() {
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let workers: Vec<_> = (0..8)
        .map(|bit| {
            let other = client.clone();
            tokio::spawn(async move { other.db_write_bool(1, 0, bit, true).await.unwrap() })
        })
        .collect();
    for worker in workers {
        worker.await.unwrap();
    }
    assert_eq!(client.db_read_byte(1, 0).await.unwrap(), 255);
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// A merged error is isolated so adjacent valid bytes retain their own outcome.
#[test]
fn merged_read_errors_fall_back_to_original_items() {
    use rs_snap7::Address;
    let peer = Peer::new(None, None);
    let client = peer.client();
    client.db_write(1, 1023, &[55]).unwrap();
    for _ in 0..2 {
        let results = client
            .read_multi_vars(&[
                Address::db(1, 1023, 1).unwrap(),
                Address::db(1, 1024, 1).unwrap(),
            ])
            .unwrap();
        assert_eq!(results[0].as_ref().unwrap(), &[55]);
        assert!(matches!(results[1], Err(Error::Item(5))));
    }
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

/// Async merged reads use the same plan cache and independent-error fallback.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_merged_read_error_fallback() {
    use rs_snap7::Address;
    let peer = Peer::new(None, None);
    let client = rs_snap7::AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    client.db_write(1, 1023, &[55]).await.unwrap();
    for _ in 0..2 {
        let results = client
            .read_multi_vars(&[
                Address::db(1, 1023, 1).unwrap(),
                Address::db(1, 1024, 1).unwrap(),
            ])
            .await
            .unwrap();
        assert_eq!(results[0].as_ref().unwrap(), &[55]);
        assert!(matches!(results[1], Err(Error::Item(5))));
    }
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Nested typed/raw calls emit one event, with callbacks outside the session guard.
#[test]
fn logical_operation_events_are_once_and_reentrant() {
    use rs_snap7::observability::OperationHook;
    let peer = Peer::new(None, None);
    let events = Arc::new(Mutex::new(Vec::new()));
    let current: Arc<Mutex<Option<Client>>> = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&events);
    let holder = Arc::clone(&current);
    let mut config = ClientConfig::new(peer.endpoint, 0, 1).unwrap();
    config.operation_hook = OperationHook::new(move |event| {
        if let Some(client) = holder.lock().unwrap().as_ref() {
            // A session probe would deadlock if the operation retained its guard.
            if event.operation != "disconnect" {
                assert_eq!(client.pdu_length().unwrap(), 128);
            }
        }
        sink.lock().unwrap().push(event);
    });
    let client = Client::connect(config).unwrap();
    *current.lock().unwrap() = Some(client.clone());
    client.db_write_int(1, 0, 42).unwrap();
    assert_eq!(client.db_read_int(1, 0).unwrap(), 42);
    assert!(client.db_write_string(1, 0, "long", 1).is_err());
    assert!(client.db_read(1, 2000, 1).is_err());
    let stats = client.operation_stats();
    assert_eq!(stats.completed, 5);
    assert_eq!(stats.failed, 2);
    assert_eq!(stats.hook_panics, 0);
    assert_eq!(stats.last.unwrap().error_code, Some(5));
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.operation)
            .collect::<Vec<_>>(),
        [
            "connect",
            "db_write_int",
            "db_read_int",
            "db_write_string",
            "db_read"
        ]
    );
    client.disconnect().unwrap();
    *current.lock().unwrap() = None;
    peer.worker.join().unwrap();
}

/// Future-local nesting does not mix operation events from concurrently joined calls.
#[cfg(feature = "async")]
#[tokio::test(flavor = "current_thread")]
async fn async_logical_events_are_not_duplicated_by_nested_calls() {
    use rs_snap7::observability::OperationHook;
    let peer = Peer::new(None, None);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut config = ClientConfig::new(peer.endpoint, 0, 1).unwrap();
    config.operation_hook = OperationHook::new(move |event| {
        sink.lock().unwrap().push(event);
    });
    let client = rs_snap7::AsyncClient::connect(config).await.unwrap();
    let (a, b) = tokio::join!(
        client.db_write_bool(1, 0, 0, true),
        client.db_write_bool(1, 0, 1, true)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(client.operation_stats().completed, 3);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.operation)
            .collect::<Vec<_>>(),
        ["connect", "db_write_bool", "db_write_bool"]
    );
    assert_eq!(client.db_read_byte(1, 0).await.unwrap(), 3);
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

/// Tags retain scalar types, batch item outcomes, and neighboring BOOL bits.
#[test]
fn tag_io_encodes_before_sending_and_preserves_neighbors() {
    use rs_snap7::tags::{DataType, Tag, Value};
    let peer = Peer::new(None, None);
    let client = peer.client();
    let integer: Tag = "DB1:2:INT".parse().unwrap();
    client.write_tag(&integer, &Value::Signed(-123)).unwrap();
    assert_eq!(client.read_tag(&integer).unwrap(), Value::Signed(-123));
    client.db_write(1, 10, &[0xaa, 0x55, 0xff]).unwrap();
    let bits: Tag = "DB1:10.3:BOOL[3]".parse().unwrap();
    client
        .write_tag(
            &bits,
            &Value::Array(vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false),
            ]),
        )
        .unwrap();
    assert_eq!(client.db_read(1, 10, 3).unwrap(), [0xa2, 0x5d, 0xf7]);
    assert!(client.write_tag(&integer, &Value::Signed(99999)).is_err());
    let invalid = Tag::from_access_string("8A0E0001.A", DataType::Int, 1, 0).unwrap();
    assert!(client.read_tag(&invalid).is_err());
    let missing: Tag = "DB1:1024:BYTE".parse().unwrap();
    let result = client.read_tags(&[integer, missing]).unwrap();
    assert!(result[0].is_ok());
    assert!(matches!(result[1], Err(Error::Item(5))));
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

#[cfg(feature = "async")]
#[tokio::test]
async fn async_tag_io_matches_blocking_and_rejects_before_network() {
    use rs_snap7::{
        AsyncClient,
        tags::{Tag, Value},
    };
    let peer = Peer::new(None, None);
    let client = AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let tag: Tag = "DB1:0:REAL[3]".parse().unwrap();
    let value = Value::Array(vec![Value::Real(1.5), Value::Real(-2.0), Value::Real(3.0)]);
    client.write_tag(&tag, &value).await.unwrap();
    assert_eq!(
        client.read_tags(&[tag]).await.unwrap().remove(0).unwrap(),
        value
    );
    client.db_write(1, 20, &[0xf0]).await.unwrap();
    let bit: Tag = "DB1:20.0:BOOL".parse().unwrap();
    client.write_tag(&bit, &Value::Bool(true)).await.unwrap();
    assert_eq!(client.db_read(1, 20, 1).await.unwrap(), [0xf1]);
    assert!(client.write_tag(&bit, &Value::Signed(1)).await.is_err());
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}

#[test]
fn db_rows_selective_fields_and_dirty_writes_match_remote_memory() {
    use rs_snap7::{
        Area,
        db::{Db, DbConfig, Layout},
        tags::Value,
    };
    let peer = Peer::new(None, None);
    let client = peer.client();
    let layout = Layout::parse("0 id INT\n2 flag BOOL\n2.7 other BOOL\n4 value REAL", 0).unwrap();
    let mut db = Db::new(
        DbConfig {
            area: Area::Db,
            db_number: 1,
            start: 100,
            row_size: 8,
            row_gap: 2,
            rows: 2,
            writable_start: 2,
        },
        layout,
        vec![0; 18],
    )
    .unwrap();
    db.set(1, "value", &Value::Real(12.5)).unwrap();
    db.write_row(&client, 1, true).unwrap();
    assert!(db.unchanged(1).unwrap());
    assert_eq!(client.db_read_real(1, 114).unwrap(), 12.5);
    client.db_write(1, 102, &[0x80]).unwrap();
    db.set(0, "flag", &Value::Bool(true)).unwrap();
    db.write_fields(&client, 0, &["flag"]).unwrap();
    assert_eq!(client.db_read(1, 102, 1).unwrap(), [0x81]);
    assert!(db.write_fields(&client, 0, &["id"]).is_err());
    db.read_fields(&client, 0, &["other"]).unwrap();
    assert_eq!(db.get(0, "other").unwrap(), Value::Bool(true));
    db.read_row(&client, 1).unwrap();
    assert_eq!(db.get(1, "value").unwrap(), Value::Real(12.5));
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

#[test]
fn db_failed_dirty_write_does_not_acknowledge_local_changes() {
    use rs_snap7::{
        Area,
        db::{Db, DbConfig, Layout},
        tags::Value,
    };
    let peer = Peer::new(Some(1), None);
    let client = peer.client();
    let mut db = Db::new(
        DbConfig {
            area: Area::Db,
            db_number: 1,
            start: 0,
            row_size: 2,
            row_gap: 0,
            rows: 1,
            writable_start: 0,
        },
        Layout::parse("0 value INT", 0).unwrap(),
        vec![0; 2],
    )
    .unwrap();
    db.set(0, "value", &Value::Signed(123)).unwrap();
    assert!(db.write_row(&client, 0, true).is_err());
    assert!(!db.unchanged(0).unwrap());
    client.disconnect().unwrap();
    peer.worker.join().unwrap();
}

#[cfg(feature = "async")]
#[tokio::test]
async fn async_db_fields_and_rows_use_native_client() {
    use rs_snap7::{
        Area, AsyncClient,
        db::{Db, DbConfig, Layout},
        tags::Value,
    };
    let peer = Peer::new(None, None);
    let client = AsyncClient::connect(ClientConfig::new(peer.endpoint, 0, 1).unwrap())
        .await
        .unwrap();
    let mut db = Db::new(
        DbConfig {
            area: Area::Db,
            db_number: 1,
            start: 0,
            row_size: 4,
            row_gap: 0,
            rows: 1,
            writable_start: 0,
        },
        Layout::parse("0 value REAL", 0).unwrap(),
        vec![0; 4],
    )
    .unwrap();
    db.set(0, "value", &Value::Real(3.5)).unwrap();
    db.write_fields_async(&client, 0, &["value"]).await.unwrap();
    db.read_row_async(&client, 0).await.unwrap();
    assert!(db.unchanged(0).unwrap());
    assert_eq!(db.get(0, "value").unwrap(), Value::Real(3.5));
    client.disconnect().await.unwrap();
    peer.worker.join().unwrap();
}
