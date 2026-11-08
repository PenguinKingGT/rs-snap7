//! Independently framed local peer helpers; no PLC or Python dependency.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

/// Read one independently parsed TPKT payload under a test deadline.
pub fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..2], &[3, 0]);
    let mut data = vec![0; usize::from(u16::from_be_bytes([header[2], header[3]])) - 4];
    stream.read_exact(&mut data).unwrap();
    data
}

/// Frame bytes directly, without using the Rust library's transport encoder.
pub fn send_frame(stream: &mut TcpStream, payload: &[u8]) {
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = vec![3, 0];
    bytes.extend_from_slice(&((payload.len() + 4) as u16).to_be_bytes());
    bytes.extend_from_slice(payload);
    stream.write_all(&bytes).unwrap();
}

/// Respond to COTP and setup jobs with independently assembled wire bytes.
pub fn negotiate(stream: &mut TcpStream, pdu: u16) {
    let cr = read_frame(stream);
    assert_eq!(cr[1], 0xe0);
    send_frame(stream, &[9, 0xd0, 0, 1, 0, 7, 0, 0xc0, 1, 10]);
    let setup = read_frame(stream);
    assert_eq!(
        &setup[3..],
        &[
            0x32, 1, 0, 0, 0, 1, 0, 8, 0, 0, 0xf0, 0, 0, 1, 0, 1, 1, 0xe0
        ]
    );
    let mut reply = vec![
        2, 0xf0, 0x80, 0x32, 3, 0, 0, 0, 1, 0, 8, 0, 0, 0, 0, 0xf0, 0, 0, 1, 0, 1,
    ];
    reply.extend_from_slice(&pdu.to_be_bytes());
    send_frame(stream, &reply);
}
