//! Wire-level limiter rejection, burst accounting, and deadline guarantees.

mod common;
use common::*;
#[cfg(feature = "blocking")]
use rs_snap7::blocking::Client;
use rs_snap7::{ClientConfig, Error, policy::*};
use std::{net::TcpListener, thread};

/// Setup consumes one S7 grant; the next job is rejected before any bytes are sent.
#[test]
#[cfg(feature = "blocking")]
fn rejection_and_deadline_do_not_send_or_corrupt_session() {
    for behavior in [RateBehavior::Reject, RateBehavior::Block] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        config.rate_limit = RateLimit {
            max_per_second: 0.01,
            behavior,
            ..Default::default()
        };
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 480);
            assert_eq!(read_frame(&mut stream)[1], 0x80);
        });
        let client = Client::connect(config).unwrap();
        let error = client.db_write(1, 0, &[1]).unwrap_err();
        match behavior {
            RateBehavior::Reject => assert!(matches!(error, Error::RateLimited)),
            RateBehavior::Block => assert!(matches!(error, Error::DeadlineBeforeSend)),
        }
        assert!(client.is_connected().unwrap());
        client.disconnect().unwrap();
        peer.join().unwrap();
    }
}

/// Async limiting has identical accounting without blocking the executor or sending a job.
#[tokio::test(flavor = "current_thread")]
async fn async_rejection_and_deadline_preserve_session() {
    for behavior in [RateBehavior::Reject, RateBehavior::Block] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = ClientConfig::new(listener.local_addr().unwrap(), 0, 1).unwrap();
        config.rate_limit = RateLimit {
            max_per_second: 0.01,
            behavior,
            ..Default::default()
        };
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            negotiate(&mut stream, 480);
            assert_eq!(read_frame(&mut stream)[1], 0x80);
        });
        let client = rs_snap7::Client::connect(config).await.unwrap();
        let error = client.db_write(1, 0, &[1]).await.unwrap_err();
        match behavior {
            RateBehavior::Reject => assert!(matches!(error, Error::RateLimited)),
            RateBehavior::Block => assert!(matches!(error, Error::DeadlineBeforeSend)),
        }
        assert!(client.is_connected().await);
        client.disconnect().await.unwrap();
        peer.join().unwrap();
    }
}
