//! Per-connection protocol engine. Session transfer state never crosses clients.

use super::{EventKind, Shared};
use crate::{
    Address, Area, BlockType, CpuState, Error, Result, WordLen, codec,
    protocol::{self, Pdu, PduType},
    transport::accept_connection,
};
use std::{
    net::{SocketAddr, TcpStream},
    sync::Arc,
};

/// A single bounded catalog/SZL continuation.
struct Continuation {
    group: u8,
    sub: u8,
    sequence: u8,
    bytes: Vec<u8>,
    offset: usize,
}
/// An isolated upload snapshot, bounded by its five-digit wire size.
struct Upload {
    bytes: Vec<u8>,
    offset: usize,
}
/// PLC-driven download state; registration occurs only after complete transfer.
struct Download {
    number: u16,
    size: usize,
    bytes: Vec<u8>,
    reference: u16,
    ending: bool,
}
/// Negotiated application state for one connected client.
struct Engine<'a> {
    shared: &'a Shared,
    peer: SocketAddr,
    pdu: usize,
    continuation: Option<Continuation>,
    upload: Option<Upload>,
    download: Option<Download>,
}

/// Complete COTP/setup and drive independent client jobs until disconnect/failure.
pub(super) fn serve(stream: TcpStream, shared: &Arc<Shared>, peer: SocketAddr) -> Result<()> {
    let mut connection = accept_connection(stream, shared.config.timeout, 65535, None)?;
    let setup = Pdu::decode(&connection.receive()?)?;
    if setup.kind != PduType::Job
        || setup.parameters.len() != 8
        || setup.parameters[..2] != [0xf0, 0]
        || !setup.data.is_empty()
    {
        return Err(Error::Protocol("server requires setup communication first"));
    }
    let requested = codec::read::<u16>(&setup.parameters, 6)?;
    if requested < 128 {
        return Err(Error::Protocol("invalid setup PDU size"));
    }
    let pdu = requested.min(shared.config.pdu_length);
    let mut parameters = vec![0xf0, 0, 0, 1, 0, 1];
    parameters.extend_from_slice(&pdu.to_be_bytes());
    connection.send(&ack(&setup, parameters, vec![]).encode()?)?;
    let mut engine = Engine {
        shared,
        peer,
        pdu: usize::from(pdu),
        continuation: None,
        upload: None,
        download: None,
    };
    loop {
        let raw = match connection.receive() {
            Ok(bytes) => bytes,
            Err(Error::Disconnected) => return Ok(()),
            Err(e) => return Err(e),
        };
        if raw.len() > engine.pdu {
            return Err(Error::Protocol("request exceeds negotiated PDU"));
        }
        let request = Pdu::decode(&raw)?;
        let replies = engine.process(&request)?;
        for reply in replies {
            let bytes = reply.encode()?;
            if bytes.len() > engine.pdu {
                return Err(Error::Protocol("response exceeds negotiated PDU"));
            }
            connection.send(&bytes)?;
        }
    }
}

/// Build a canonical acknowledgement retaining the caller reference.
fn ack(job: &Pdu, parameters: Vec<u8>, data: Vec<u8>) -> Pdu {
    Pdu {
        kind: PduType::AckData,
        reference: job.reference,
        error: 0,
        parameters,
        data,
    }
}
/// Build a whole-job error with its unmodified protocol code.
fn refusal(job: &Pdu, code: u16) -> Pdu {
    Pdu {
        kind: PduType::Ack,
        reference: job.reference,
        error: code,
        parameters: vec![],
        data: vec![],
    }
}
/// Convert one S7 ANY wire specification to an independently validated address.
fn address(bytes: &[u8]) -> Result<Address> {
    if bytes.len() != 12 || bytes[..3] != [0x12, 0x0a, 0x10] {
        return Err(Error::Item(6));
    }
    let word = WordLen::try_from(bytes[3]).map_err(|_| Error::Item(6))?;
    let area = Area::try_from(bytes[8]).map_err(|_| Error::Item(6))?;
    let offset = u32::from_be_bytes([0, bytes[9], bytes[10], bytes[11]]);
    let start = if word == WordLen::Bit || word.is_indexed() {
        offset
    } else {
        if !offset.is_multiple_of(8) {
            return Err(Error::Item(5));
        }
        offset / 8
    };
    Address::new(
        area,
        codec::read(bytes, 6)?,
        start,
        word,
        codec::read(bytes, 4)?,
    )
    .map_err(|_| Error::Item(5))
}
/// Interpret the byte span touched by a validated address.
fn memory_range(address: Address) -> std::ops::Range<usize> {
    let start = address.start() as usize;
    if address.word_len() == WordLen::Bit {
        start / 8..(start + usize::from(address.count())).div_ceil(8)
    } else if address.word_len().is_indexed() {
        start * 2..start * 2 + address.byte_len()
    } else {
        start..start + address.byte_len()
    }
}
/// Read memory without holding the registry during the area lock.
fn read(shared: &Shared, address: Address) -> Result<Vec<u8>> {
    let handle = shared.memory(address.area(), address.db_number())?;
    let memory = handle
        .0
        .read()
        .map_err(|_| Error::Protocol("poisoned memory"))?;
    let range = memory_range(address);
    let bytes = memory.get(range.clone()).ok_or(Error::Item(5))?;
    if address.word_len() == WordLen::Bit {
        Ok((0..usize::from(address.count()))
            .map(|i| {
                let bit = address.start() as usize + i;
                (bytes[bit / 8 - range.start] >> (bit % 8)) & 1
            })
            .collect())
    } else {
        Ok(bytes.to_vec())
    }
}
/// Validate and apply an entire item atomically under its memory write lock.
fn write(shared: &Shared, address: Address, data: &[u8]) -> Result<()> {
    if data.len() != address.byte_len()
        || (address.word_len() == WordLen::Bit && data.iter().any(|b| *b > 1))
    {
        return Err(Error::Item(7));
    }
    let handle = shared.memory(address.area(), address.db_number())?;
    let mut memory = handle
        .0
        .write()
        .map_err(|_| Error::Protocol("poisoned memory"))?;
    let range = memory_range(address);
    let target = memory.get_mut(range.clone()).ok_or(Error::Item(5))?;
    if address.word_len() == WordLen::Bit {
        for (i, value) in data.iter().enumerate() {
            let bit = address.start() as usize + i;
            let byte = &mut target[bit / 8 - range.start];
            let mask = 1 << (bit % 8);
            *byte = (*byte & !mask) | (*value << (bit % 8));
        }
    } else {
        target.copy_from_slice(data);
    }
    Ok(())
}
impl Engine<'_> {
    /// Dispatch one bounded, decoded request; unsupported services get explicit codes.
    fn process(&mut self, job: &Pdu) -> Result<Vec<Pdu>> {
        if self.download.is_some() {
            return self.download_reply(job);
        }
        if job.kind == PduType::UserData {
            return Ok(vec![self.userdata(job)?]);
        }
        if job.kind != PduType::Job || job.parameters.is_empty() {
            return Err(Error::Protocol("expected server job"));
        }
        let function = job.parameters[0];
        let response = match function {
            4 | 5 => self.memory(job)?,
            0x28 | 0x29 => self.control(job)?,
            0x1d..=0x1f => self.upload(job)?,
            0x1a => return self.offer_download(job),
            _ => refusal(job, 0x8104),
        };
        Ok(vec![response])
    }

    /// Parse all write payloads before mutating any registered memory.
    fn memory(&self, job: &Pdu) -> Result<Pdu> {
        let parameters = &job.parameters;
        if parameters.len() < 2 {
            return Err(Error::Protocol("missing memory item count"));
        }
        let count = usize::from(parameters[1]);
        if !(1..=20).contains(&count) || parameters.len() != 2 + count * 12 {
            return Err(Error::Protocol("invalid memory item count"));
        }
        let addresses: Vec<_> = parameters[2..].chunks_exact(12).map(address).collect();
        let writing = parameters[0] == 5;
        let mut inputs = Vec::new();
        if writing {
            let mut cursor = 0;
            for (index, address) in addresses.iter().enumerate() {
                let header = job
                    .data
                    .get(cursor..cursor + 4)
                    .ok_or(Error::Protocol("truncated write item header"))?;
                if header[0] != 0 || !matches!(header[1], 3 | 4 | 5 | 7 | 9) {
                    return Err(Error::Protocol("invalid write data header"));
                }
                let length = usize::from(codec::read::<u16>(header, 2)?);
                let length = if matches!(header[1], 3 | 9) {
                    length
                } else {
                    length.div_ceil(8)
                };
                cursor += 4;
                let data = job
                    .data
                    .get(cursor..cursor + length)
                    .ok_or(Error::Protocol("truncated write item"))?;
                if let Ok(address) = address {
                    let expected = if address.word_len() == WordLen::Bit {
                        3
                    } else if address.word_len().is_indexed() {
                        9
                    } else {
                        4
                    };
                    if header[1] != expected {
                        return Err(Error::Protocol(
                            "write transport size does not match address",
                        ));
                    }
                }
                inputs.push(data);
                cursor += length;
                if index + 1 < count && length % 2 == 1 {
                    if job.data.get(cursor) != Some(&0) {
                        return Err(Error::Protocol("missing write item pad"));
                    }
                    cursor += 1;
                }
            }
            if cursor != job.data.len() {
                return Err(Error::Protocol("trailing write bytes"));
            }
        } else if !job.data.is_empty() {
            return Err(Error::Protocol("read request has data"));
        }
        let read_budget = 14
            + addresses
                .iter()
                .map(|address| {
                    4 + address
                        .as_ref()
                        .map_or(0, |a| a.byte_len().next_multiple_of(2))
                })
                .sum::<usize>();
        if !writing && read_budget > self.pdu {
            return Ok(refusal(job, 0x8500));
        }
        let mut data = Vec::new();
        for (index, address) in addresses.into_iter().enumerate() {
            let result = address.and_then(|a| {
                if writing {
                    write(self.shared, a, inputs[index]).map(|_| (a, vec![]))
                } else {
                    read(self.shared, a).map(|data| (a, data))
                }
            });
            match result {
                Ok((address, bytes)) if !writing => {
                    let transport = if address.word_len() == WordLen::Bit {
                        3
                    } else if address.word_len().is_indexed() {
                        9
                    } else {
                        4
                    };
                    let length = if transport == 4 {
                        bytes.len() * 8
                    } else {
                        bytes.len()
                    };
                    if length > 65535 {
                        return Ok(refusal(job, 0x8500));
                    }
                    data.extend_from_slice(&[0xff, transport]);
                    data.extend_from_slice(&(length as u16).to_be_bytes());
                    data.extend_from_slice(&bytes);
                    if index + 1 < count && bytes.len() % 2 == 1 {
                        data.push(0);
                    }
                }
                Ok(_) => data.push(0xff),
                Err(error) => {
                    let code = match error {
                        Error::Item(code) => code,
                        _ => 1,
                    };
                    if writing {
                        data.push(code);
                    } else {
                        data.extend_from_slice(&[code, 0, 0, 0]);
                    }
                }
            }
        }
        self.shared.emit(
            if writing {
                EventKind::Write
            } else {
                EventKind::Read
            },
            self.peer,
            parameters[0],
        );
        Ok(ack(job, parameters[..2].to_vec(), data))
    }

    /// Emulate CPU state and PI acknowledgements; no actual program execution occurs.
    fn control(&self, job: &Pdu) -> Result<Pdu> {
        use protocol::management::{Control, control_request};
        if job.parameters.ends_with(b"_DELE") {
            let p = &job.parameters;
            if p.len() != 22 {
                return Ok(refusal(job, 0x8104));
            }
            let code = std::str::from_utf8(&p[9..11])
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok());
            let number = decimal(&p[11..16]);
            if code != Some(BlockType::Db as u8) || number.is_err() {
                return Ok(refusal(job, 0x8104));
            }
            let removed = self
                .shared
                .memory
                .write()
                .map_err(|_| Error::Protocol("poisoned registry"))?
                .remove(&(Area::Db as u8, number? as u16));
            return Ok(if removed.is_some() {
                ack(job, vec![0x28], vec![])
            } else {
                refusal(job, 0x8104)
            });
        }
        let operation = [
            Control::Stop,
            Control::HotStart,
            Control::ColdStart,
            Control::Compress,
            Control::CopyRamToRom,
        ]
        .into_iter()
        .find(|op| control_request(0, *op).parameters == job.parameters);
        let Some(operation) = operation else {
            return Ok(refusal(job, 0x8104));
        };
        if !job.data.is_empty() {
            return Ok(refusal(job, 0x8104));
        }
        match operation {
            Control::Stop => {
                *self
                    .shared
                    .cpu
                    .lock()
                    .map_err(|_| Error::Protocol("poisoned CPU"))? = CpuState::Stop
            }
            Control::HotStart | Control::ColdStart => {
                *self
                    .shared
                    .cpu
                    .lock()
                    .map_err(|_| Error::Protocol("poisoned CPU"))? = CpuState::Run
            }
            _ => {}
        }
        self.shared
            .emit(EventKind::Control, self.peer, job.parameters[0]);
        Ok(ack(job, vec![operation.function()], vec![]))
    }

    /// Validate USERDATA envelopes and isolate continuation state per session.
    fn userdata(&mut self, job: &Pdu) -> Result<Pdu> {
        let p = &job.parameters;
        if !matches!(p.len(), 8 | 12)
            || p[..3] != [0, 1, 0x12]
            || p[3] as usize + 4 != p.len()
            || p[5] & 0xf0 != 0x40
            || job.data.len() < 4
        {
            return Err(Error::Protocol("invalid USERDATA request"));
        }
        let (group, sub) = (p[5] & 15, p[6]);
        if p.len() == 12 {
            let continuation = self
                .continuation
                .as_ref()
                .ok_or(Error::Protocol("unexpected continuation"))?;
            if (group, sub, p[7]) != (continuation.group, continuation.sub, continuation.sequence)
                || job.data != [0x0a, 0, 0, 0]
            {
                return Err(Error::Protocol("continuation identity mismatch"));
            }
            return self.next_fragment(job);
        }
        if codec::read::<u16>(&job.data, 2)? as usize != job.data.len() - 4 {
            return Err(Error::Protocol("USERDATA payload length mismatch"));
        }
        let payload = &job.data[4..];
        let data = match (group, sub) {
            (4, 1) if payload.len() == 4 => self.szl(payload),
            (3, 1..=3) => self.catalog(sub, payload),
            (7, 1) => {
                let mut bytes = vec![0, 0x19, 0, 0, 0, 0, 0, 0, 0, 0];
                codec::datetime::set_dt(
                    &mut bytes,
                    2,
                    *self
                        .shared
                        .clock
                        .lock()
                        .map_err(|_| Error::Protocol("poisoned clock"))?,
                )?;
                Ok(bytes)
            }
            (7, 2) if payload.len() == 10 && payload[..2] == [0, 0x19] => {
                let value = codec::datetime::get_dt(payload, 2)?;
                *self
                    .shared
                    .clock
                    .lock()
                    .map_err(|_| Error::Protocol("poisoned clock"))? = value;
                Ok(vec![])
            }
            (5, 1) if payload.len() == 8 => Ok(vec![]),
            (5, 2) if payload.is_empty() => Ok(vec![]),
            _ => Err(Error::Plc(0x8104)),
        };
        match data {
            Ok(bytes) => {
                self.continuation = Some(Continuation {
                    group,
                    sub,
                    sequence: 1,
                    bytes,
                    offset: 0,
                });
                self.next_fragment(job)
            }
            Err(Error::Plc(code)) => {
                let code = if code == 0 { 0x8104 } else { code };
                Ok(user_reply(job, group, sub, 0, false, code, &[], false))
            }
            Err(Error::Item(_)) => Ok(user_reply(job, group, sub, 0, false, 0x8104, &[], false)),
            Err(error) => Err(error),
        }
    }

    /// Emit bounded chunks; the client continuation echoes our sequence.
    fn next_fragment(&mut self, job: &Pdu) -> Result<Pdu> {
        let state = self
            .continuation
            .as_mut()
            .ok_or(Error::Protocol("no continuation"))?;
        let end = (state.offset + self.pdu - 26).min(state.bytes.len());
        let more = end < state.bytes.len();
        let reply = user_reply(
            job,
            state.group,
            state.sub,
            state.sequence,
            more,
            0,
            &state.bytes[state.offset..end],
            state.bytes.is_empty() && (state.group == 5 || state.group == 7 && state.sub == 2),
        );
        state.offset = end;
        state.sequence = state.sequence.wrapping_add(1);
        if !more {
            self.continuation = None;
        } else {
            // Follow-up identifies the preceding reply, not the next sequence.
            state.sequence = state.sequence.wrapping_sub(1);
        }
        Ok(reply)
    }

    /// Return registered SZLs or explicitly identified emulator defaults.
    fn szl(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let id = codec::read::<u16>(payload, 0)?;
        let index = codec::read::<u16>(payload, 2)?;
        let body = if let Some(body) = self
            .shared
            .szls
            .read()
            .map_err(|_| Error::Protocol("poisoned SZLs"))?
            .get(&(id, index))
            .cloned()
        {
            body
        } else {
            match (id, index) {
                (0x0424, 0) => {
                    let mut body = vec![0, 4, 0, 1, 0, 0, 0, 0];
                    body[7] = match *self
                        .shared
                        .cpu
                        .lock()
                        .map_err(|_| Error::Protocol("poisoned CPU"))?
                    {
                        CpuState::Run => 8,
                        CpuState::Stop => 4,
                        _ => 0,
                    };
                    body
                }
                (0x001c, 0) => {
                    let mut body = vec![0; 208];
                    body[6..17].copy_from_slice(b"Rust S7 SIM");
                    body[40..48].copy_from_slice(b"Emulator");
                    body
                }
                (0x0131, 0) => {
                    let mut body = self.shared.config.pdu_length.to_be_bytes().to_vec();
                    body.extend_from_slice(&(self.shared.config.max_clients as u16).to_be_bytes());
                    body.extend_from_slice(&[0, 0, 0, 0]);
                    body
                }
                (0x0232, 0) => vec![0; 10],
                (0x00a0, 0) => vec![],
                (0, 0) => {
                    let ids = [0x001cu16, 0x0131, 0x0232, 0x0424, 0x00a0, 0x0011];
                    let mut body = vec![0, 2, 0, 6];
                    for id in ids {
                        body.extend_from_slice(&id.to_be_bytes());
                    }
                    body
                }
                (0x0011, 0) => {
                    let mut body = vec![0, 28, 0, 1, 0, 1];
                    body.extend_from_slice(b"RS-SNAP7 EMULATOR    ");
                    body.resize(32, 0);
                    body
                }
                _ => return Err(Error::Plc(0x8104)),
            }
        };
        let mut result = payload.to_vec();
        result.extend(body);
        Ok(result)
    }

    /// Catalog only registered DBs; no synthetic OB/FB program contents are invented.
    fn catalog(&self, sub: u8, payload: &[u8]) -> Result<Vec<u8>> {
        let registry = self
            .shared
            .memory
            .read()
            .map_err(|_| Error::Protocol("poisoned registry"))?;
        match sub {
            1 => {
                let count = registry
                    .keys()
                    .filter(|(area, _)| *area == Area::Db as u8)
                    .count();
                let mut bytes = vec![];
                for kind in [
                    BlockType::Ob,
                    BlockType::Fb,
                    BlockType::Fc,
                    BlockType::Db,
                    BlockType::Sdb,
                    BlockType::Sfc,
                    BlockType::Sfb,
                ] {
                    bytes.extend_from_slice(&[0x30, kind as u8]);
                    bytes.extend_from_slice(
                        &(if kind == BlockType::Db {
                            count as u16
                        } else {
                            0
                        })
                        .to_be_bytes(),
                    );
                }
                Ok(bytes)
            }
            2 if payload.len() == 2 && payload[0] == 0x30 => {
                let mut bytes = vec![];
                if payload[1] == BlockType::Db as u8 {
                    for (area, db) in registry.keys() {
                        if *area == Area::Db as u8 {
                            bytes.extend_from_slice(&db.to_be_bytes());
                            bytes.extend_from_slice(&[0, 5]);
                        }
                    }
                }
                Ok(bytes)
            }
            3 if payload.len() == 8 && payload[..2] == [0x30, BlockType::Db as u8] => {
                let db = u16::try_from(decimal(&payload[2..7])?).map_err(|_| Error::Plc(0x8104))?;
                let memory = registry
                    .get(&(Area::Db as u8, db))
                    .cloned()
                    .ok_or(Error::Plc(0x8104))?;
                drop(registry);
                let size = memory
                    .0
                    .read()
                    .map_err(|_| Error::Protocol("poisoned memory"))?
                    .len();
                if size > 65536 {
                    return Err(Error::Plc(0x8500));
                }
                let mut bytes = vec![0; 78];
                bytes[1] = BlockType::Db as u8;
                bytes[10] = 5;
                codec::write(&mut bytes, 12, db)?;
                codec::write(&mut bytes, 14, (size + 36) as u32)?;
                codec::write(&mut bytes, 40, size as u16)?;
                Ok(bytes)
            }
            _ => Err(Error::Plc(0x8104)),
        }
    }

    /// Upload a frozen memory snapshot with a compact 36-byte block header.
    fn upload(&mut self, job: &Pdu) -> Result<Pdu> {
        match job.parameters[0] {
            0x1d => {
                let p = &job.parameters;
                if p.len() != 18
                    || p[8] != 9
                    || p[9..12] != [b'_', b'0', BlockType::Db as u8]
                    || p[17] != b'A'
                    || self.upload.is_some()
                {
                    return Ok(refusal(job, 0x8104));
                }
                let number = u16::try_from(decimal(&p[12..17])?)
                    .map_err(|_| Error::Protocol("invalid block number"))?;
                let memory = match self.shared.memory(Area::Db, number) {
                    Ok(m) => m,
                    Err(_) => return Ok(refusal(job, 0x8104)),
                };
                let content = memory.snapshot()?;
                if content.len() > 65535 {
                    return Ok(refusal(job, 0x8500));
                }
                let mut bytes = vec![0; 36];
                bytes[2] = 1;
                bytes[4] = 5;
                bytes[5] = 0x0a;
                codec::write(&mut bytes, 6, number)?;
                codec::write(&mut bytes, 8, (content.len() + 36) as u32)?;
                codec::write(&mut bytes, 34, content.len() as u16)?;
                bytes.extend(content);
                let mut p = vec![0x1d, 0, 0, 0, 0, 0, 0, 1, 0, 0, 5];
                p.extend_from_slice(format!("{:05}", bytes.len()).as_bytes());
                self.upload = Some(Upload { bytes, offset: 0 });
                Ok(ack(job, p, vec![]))
            }
            0x1e => {
                if job.parameters != [0x1e, 0, 0, 0, 0, 0, 0, 1] {
                    return Ok(refusal(job, 0x8104));
                }
                let Some(upload) = self.upload.as_mut() else {
                    return Ok(refusal(job, 0x8104));
                };
                if upload.offset >= upload.bytes.len() {
                    return Ok(refusal(job, 0x8104));
                }
                let end = (upload.offset + self.pdu - 18).min(upload.bytes.len());
                let mut data = ((end - upload.offset) as u16).to_be_bytes().to_vec();
                data.extend_from_slice(&[0, 0xfb]);
                data.extend_from_slice(&upload.bytes[upload.offset..end]);
                upload.offset = end;
                Ok(ack(
                    job,
                    vec![0x1e, u8::from(end < upload.bytes.len())],
                    data,
                ))
            }
            0x1f => {
                if job.parameters != [0x1f, 0, 0, 0, 0, 0, 0, 1]
                    || self
                        .upload
                        .as_ref()
                        .is_none_or(|upload| upload.offset != upload.bytes.len())
                {
                    return Ok(refusal(job, 0x8104));
                }
                self.upload = None;
                Ok(ack(job, vec![0x1f], vec![]))
            }
            _ => unreachable!(),
        }
    }

    /// Accept a download offer before issuing our own correlated fragment requests.
    fn offer_download(&mut self, job: &Pdu) -> Result<Vec<Pdu>> {
        let p = &job.parameters;
        if p.len() != 32
            || p[8] != 9
            || p[9..12] != [b'_', b'0', BlockType::Db as u8]
            || p[17] != b'P'
            || p[18..20] != [13, b'1']
        {
            return Ok(vec![refusal(job, 0x8104)]);
        }
        let number = u16::try_from(decimal(&p[12..17])?)
            .map_err(|_| Error::Protocol("invalid download DB"))?;
        let size = decimal(&p[20..26])?;
        if !(36..=65571).contains(&size) {
            return Ok(vec![refusal(job, 0x8500)]);
        }
        self.download = Some(Download {
            number,
            size,
            bytes: Vec::new(),
            reference: 0x8001,
            ending: false,
        });
        Ok(vec![ack(job, vec![0x1a], vec![]), self.download_job()?])
    }

    /// Construct an unsolicited PLC job using our own reference, not the client sequence.
    fn download_job(&self) -> Result<Pdu> {
        let state = self
            .download
            .as_ref()
            .ok_or(Error::Protocol("no download state"))?;
        let mut p = vec![
            if state.ending { 0x1c } else { 0x1b },
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            9,
        ];
        p.extend(protocol::blocks::block_address(
            BlockType::Db,
            state.number,
            b'P',
        ));
        Ok(Pdu::job(state.reference, p, vec![]))
    }

    /// Stage download bytes, then publish atomically after the matching final ACK.
    fn download_reply(&mut self, reply: &Pdu) -> Result<Vec<Pdu>> {
        let state = self
            .download
            .as_mut()
            .ok_or(Error::Protocol("no download state"))?;
        if reply.reference != state.reference {
            return Err(Error::Protocol("download reference mismatch"));
        }
        if state.ending {
            protocol::blocks::check_ack(reply, 0x1c)?;
            let state = self
                .download
                .take()
                .ok_or(Error::Protocol("missing download"))?;
            let info = crate::blocks::CompactBlockInfo::decode(&state.bytes)?;
            if info.number != state.number
                || info.language != 5
                || usize::from(info.mc7_size) + 36 != state.bytes.len()
            {
                return Err(Error::Protocol("invalid downloaded DB header"));
            }
            let mut registry = self
                .shared
                .memory
                .write()
                .map_err(|_| Error::Protocol("poisoned registry"))?;
            registry.insert(
                (Area::Db as u8, state.number),
                super::Memory(Arc::new(std::sync::RwLock::new(state.bytes[36..].to_vec()))),
            );
            return Ok(vec![]);
        }
        reply.check_response()?;
        if reply.kind != PduType::AckData
            || reply.parameters.len() != 2
            || reply.parameters[0] != 0x1b
            || reply.parameters[1] > 1
            || reply.data.len() < 4
            || reply.data[2..4] != [0, 0xfb]
            || codec::read::<u16>(&reply.data, 0)? as usize != reply.data.len() - 4
        {
            return Err(Error::Protocol("invalid download fragment response"));
        }
        let chunk = &reply.data[4..];
        if chunk.is_empty() || state.bytes.len() + chunk.len() > state.size {
            return Err(Error::Protocol("download length exceeded"));
        }
        state.bytes.extend_from_slice(chunk);
        if reply.parameters[1] == 0 {
            if state.bytes.len() != state.size {
                return Err(Error::Protocol("download terminated early"));
            }
            let info = crate::blocks::CompactBlockInfo::decode(&state.bytes)?;
            if info.number != state.number
                || info.language != 5
                || usize::from(info.mc7_size) + 36 != state.bytes.len()
            {
                return Err(Error::Protocol("invalid downloaded DB representation"));
            }
            state.ending = true;
        } else if state.bytes.len() == state.size {
            return Err(Error::Protocol("download exceeded advertised length"));
        }
        state.reference = state.reference.wrapping_add(1);
        Ok(vec![self.download_job()?])
    }
}

/// Parse a bounded decimal wire field without accepting signs or whitespace.
fn decimal(bytes: &[u8]) -> Result<usize> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::Protocol("invalid decimal block field"));
    }
    bytes.iter().try_fold(0usize, |n, b| {
        n.checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(*b - b'0')))
            .ok_or(Error::Protocol("decimal field overflow"))
    })
}

/// Encode one USERDATA response with lengths and continuation metadata.
#[allow(clippy::too_many_arguments)]
fn user_reply(
    job: &Pdu,
    group: u8,
    sub: u8,
    sequence: u8,
    more: bool,
    error: u16,
    payload: &[u8],
    null: bool,
) -> Pdu {
    let mut parameters = vec![
        0,
        1,
        0x12,
        8,
        0x12,
        0x80 | group,
        sub,
        sequence,
        0,
        u8::from(more),
    ];
    parameters.extend_from_slice(&error.to_be_bytes());
    let mut data = if null {
        vec![0x0a, 0, 0, 0]
    } else {
        vec![0xff, 9]
    };
    if !null {
        data.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        data.extend_from_slice(payload);
    }
    Pdu {
        kind: PduType::UserData,
        reference: job.reference,
        error: 0,
        parameters,
        data,
    }
}

#[cfg(test)]
mod tests {
    //! Independent address layouts and pre-mutation validation.
    use super::*;
    use crate::server::{Server, ServerConfig};

    #[test]
    fn malformed_addresses_and_write_sections_cannot_mutate_memory() {
        assert!(address(&[0; 12]).is_err());
        let mut spec = Address::db(1, 0, 2).unwrap().encode();
        spec[11] = 1;
        assert!(address(&spec).is_err());
        let server = Server::start(ServerConfig::new("127.0.0.1:0".parse().unwrap())).unwrap();
        let memory = server.register_area(Area::Db, 1, vec![0xaa; 16]).unwrap();
        let engine = Engine {
            shared: &server.shared,
            peer: "127.0.0.1:1".parse().unwrap(),
            pdu: 128,
            continuation: None,
            upload: None,
            download: None,
        };
        let addresses = [Address::db(1, 0, 1).unwrap(), Address::db(1, 2, 1).unwrap()];
        let mut job =
            protocol::write_request(1, &[(addresses[0], &[1]), (addresses[1], &[2])]).unwrap();
        job.data.pop();
        assert!(engine.memory(&job).is_err());
        assert_eq!(memory.snapshot().unwrap(), vec![0xaa; 16]);
        let bit = Address::new(Area::Db, 1, 7, WordLen::Bit, 2).unwrap();
        write(&server.shared, bit, &[0, 1]).unwrap();
        assert_eq!(read(&server.shared, bit).unwrap(), [0, 1]);
        assert_eq!(memory.snapshot().unwrap()[0..2], [0x2a, 0xab]);
        assert!(write(&server.shared, bit, &[2, 1]).is_err());
    }
}
