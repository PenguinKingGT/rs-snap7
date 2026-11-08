//! Shared, I/O-free upload/download state machines for both runtime adapters.

use crate::{
    BlockType, Error, Result,
    protocol::{Pdu, blocks as wire},
};

/// Next wire action; unsolicited download jobs use receive/send rather than exchange.
pub(crate) enum Action {
    /// Send a client job with a fresh session reference, then receive its ACK.
    Exchange(Pdu),
    /// Receive an unsolicited PLC job.
    Receive,
    /// Send a reply using the PLC-supplied reference.
    Send(Pdu),
    /// All protocol stages completed.
    Done,
}

/// Internal seam shared by blocking and native async transfer drivers.
pub(crate) trait Transfer {
    /// Successful operation result.
    type Output;
    /// Consume the preceding wire result and choose the next bounded action.
    fn advance(&mut self, reply: Option<Pdu>) -> Result<Action>;
    /// Recover the successful value once the machine reaches `Done`.
    fn finish(self) -> Self::Output;
    /// Classify transfer failures, including potentially applied downloads.
    fn failure(&self, error: Error) -> Error {
        error
    }
}

/// Upload phases; all outstanding jobs belong to the same connection.
enum UploadStage {
    Start,
    Opening,
    Reading,
    Closing,
    Done,
}

/// A bounded complete-block upload, stripping the compact header only at completion.
pub(crate) struct Upload {
    kind: BlockType,
    number: u16,
    full: bool,
    stage: UploadStage,
    id: u8,
    expected: usize,
    fragments: usize,
    bytes: Vec<u8>,
}

impl Upload {
    /// Create a read-only transfer for any catalog block type.
    pub(crate) fn new(kind: BlockType, number: u16, full: bool) -> Self {
        Self {
            kind,
            number,
            full,
            stage: UploadStage::Start,
            id: 0,
            expected: 0,
            fragments: 0,
            bytes: Vec::new(),
        }
    }
}

impl Transfer for Upload {
    type Output = Vec<u8>;
    /// Validate each stage before issuing another job; no missing final fragment is accepted.
    fn advance(&mut self, reply: Option<Pdu>) -> Result<Action> {
        match self.stage {
            UploadStage::Start => {
                self.stage = UploadStage::Opening;
                Ok(Action::Exchange(wire::start_upload(
                    0,
                    self.kind,
                    self.number,
                )))
            }
            UploadStage::Opening => {
                (self.id, self.expected) = wire::parse_upload_start(&required(reply)?)?;
                if !self.full && self.expected < 36 {
                    return Err(Error::Protocol("upload lacks compact header"));
                }
                self.stage = UploadStage::Reading;
                Ok(Action::Exchange(wire::upload_job(0, self.id, false)))
            }
            UploadStage::Reading => {
                let reply = required(reply)?;
                let (bytes, last) = wire::parse_upload_fragment(&reply)?;
                self.fragments += 1;
                if self.bytes.len() + bytes.len() > self.expected
                    || (!last && (bytes.is_empty() || self.fragments >= 1000))
                {
                    return Err(Error::Protocol(
                        "upload exceeds declared size or fragment limit",
                    ));
                }
                self.bytes.extend_from_slice(bytes);
                if last {
                    if self.bytes.len() != self.expected {
                        return Err(Error::Protocol("upload total length mismatch"));
                    }
                    self.stage = UploadStage::Closing;
                }
                Ok(Action::Exchange(wire::upload_job(0, self.id, last)))
            }
            UploadStage::Closing => {
                wire::check_ack(&required(reply)?, 0x1f)?;
                self.stage = UploadStage::Done;
                Ok(Action::Done)
            }
            UploadStage::Done => Err(Error::Protocol("upload already completed")),
        }
    }
    /// Return owned bytes; raw DB upload omits the first 36 compact-header bytes.
    fn finish(mut self) -> Vec<u8> {
        if self.full {
            self.bytes
        } else {
            self.bytes.split_off(36)
        }
    }
}

/// PLC-driven download phases, including the final server-initiated completion job.
enum DownloadStage {
    Start,
    Offering,
    RequestSlice,
    Slice,
    End,
    Done,
}

/// A prevalidated download with bounded slice count and immutable input bytes.
pub(crate) struct Download<'a> {
    bytes: &'a [u8],
    number: u16,
    width: usize,
    offset: usize,
    stage: DownloadStage,
    offered: bool,
}

impl<'a> Download<'a> {
    /// Validate size/fragment budgets before the download offer can touch the PLC.
    pub(crate) fn new(bytes: &'a [u8], number: u16, pdu: u16) -> Result<Self> {
        wire::download_request(0, number, bytes)?;
        let width = usize::from(pdu)
            .checked_sub(18)
            .filter(|n| *n > 0)
            .ok_or(Error::InvalidInput("PDU too small for download"))?;
        if bytes.len().div_ceil(width) > 1000 {
            return Err(Error::InvalidInput("download exceeds 1000-fragment budget"));
        }
        Ok(Self {
            bytes,
            number,
            width,
            offset: 0,
            stage: DownloadStage::Start,
            offered: false,
        })
    }
}

impl Transfer for Download<'_> {
    type Output = ();
    /// Accept only the expected server job; the PLC's reference is echoed in each reply.
    fn advance(&mut self, reply: Option<Pdu>) -> Result<Action> {
        match self.stage {
            DownloadStage::Start => {
                self.stage = DownloadStage::Offering;
                Ok(Action::Exchange(wire::download_request(
                    0,
                    self.number,
                    self.bytes,
                )?))
            }
            DownloadStage::Offering => {
                wire::check_ack(&required(reply)?, 0x1a)?;
                self.offered = true;
                self.stage = DownloadStage::Slice;
                Ok(Action::Receive)
            }
            DownloadStage::RequestSlice => {
                self.stage = DownloadStage::Slice;
                Ok(Action::Receive)
            }
            DownloadStage::Slice => {
                let job = required(reply)?;
                wire::check_download_job(&job, 0x1b, self.number)?;
                let end = (self.offset + self.width).min(self.bytes.len());
                let response = wire::download_fragment(
                    job.reference,
                    end == self.bytes.len(),
                    &self.bytes[self.offset..end],
                )?;
                self.offset = end;
                self.stage = if end == self.bytes.len() {
                    DownloadStage::End
                } else {
                    DownloadStage::RequestSlice
                };
                Ok(Action::Send(response))
            }
            DownloadStage::End => {
                if reply.is_none() {
                    return Ok(Action::Receive);
                }
                let job = required(reply)?;
                wire::check_download_job(&job, 0x1c, self.number)?;
                self.stage = DownloadStage::Done;
                Ok(Action::Send(wire::download_end(job.reference)))
            }
            DownloadStage::Done => Ok(Action::Done),
        }
    }
    /// Successful completion has no byte-count sentinel.
    fn finish(self) {}
    /// Preserve initial explicit refusals, but never imply that an interrupted download was unapplied.
    fn failure(&self, error: Error) -> Error {
        if self.offered
            || matches!(
                error,
                Error::Io(_)
                    | Error::Timeout
                    | Error::Protocol(_)
                    | Error::LostReference { .. }
                    | Error::StaleReference { .. }
            )
        {
            Error::AmbiguousOperation {
                operation: "download",
                source: Box::new(error),
            }
        } else {
            error
        }
    }
}

/// A missing response signals a driver/state mismatch rather than a panic.
fn required(reply: Option<Pdu>) -> Result<Pdu> {
    reply.ok_or(Error::Protocol("missing block-transfer response"))
}

/// USERDATA collection shares the transfer driver and its cancellation/size bounds.
pub(crate) struct Catalog {
    job: Option<Pdu>,
    subfunction: u8,
    fragments: usize,
    bytes: Vec<u8>,
}

impl Catalog {
    /// Collect an initial block-catalog request and any continuation payloads.
    pub(crate) fn new(job: Pdu, subfunction: u8) -> Self {
        Self {
            job: Some(job),
            subfunction,
            fragments: 0,
            bytes: Vec::new(),
        }
    }
}

impl Transfer for Catalog {
    type Output = Vec<u8>;
    /// Bound both continuation count and total bytes, preserving split records until final decoding.
    fn advance(&mut self, reply: Option<Pdu>) -> Result<Action> {
        if let Some(job) = self.job.take() {
            return Ok(Action::Exchange(job));
        }
        let fragment =
            crate::protocol::parse_userdata(&required(reply)?, 3, self.subfunction, false)?;
        self.fragments += 1;
        if self.bytes.len() + fragment.payload.len() > 1024 * 1024
            || (fragment.more && (fragment.payload.is_empty() || self.fragments >= 101))
        {
            return Err(Error::Protocol("catalog continuation exceeds limits"));
        }
        self.bytes.extend_from_slice(&fragment.payload);
        if fragment.more {
            Ok(Action::Exchange(crate::protocol::userdata_followup(
                0,
                3,
                self.subfunction,
                fragment.sequence,
            )))
        } else {
            Ok(Action::Done)
        }
    }
    /// Return the complete service payload with no USERDATA wrapper.
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Errors from validation/permit acquisition do not start a remote file transaction.
pub(crate) fn not_sent(error: &Error) -> bool {
    matches!(
        error,
        Error::InvalidInput(_)
            | Error::Disconnected
            | Error::RateLimited
            | Error::DeadlineBeforeSend
    )
}

#[cfg(test)]
mod tests {
    //! Deterministic transfer-budget and phase-ordering tests without sockets.
    use super::*;
    use crate::protocol::PduType;

    /// Independently assemble an acknowledgement model for pure state-machine inputs.
    fn ack(parameters: Vec<u8>, data: Vec<u8>) -> Pdu {
        Pdu {
            kind: PduType::AckData,
            reference: 1,
            error: 0,
            parameters,
            data,
        }
    }
    /// A syntactically valid start response with a five-digit size.
    fn opening(size: usize) -> Pdu {
        let mut params = vec![0x1d, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0];
        params.extend_from_slice(format!("{size:05}").as_bytes());
        ack(params, vec![])
    }
    /// Fragments may not extend beyond the advertised total or run indefinitely.
    #[test]
    fn upload_length_final_marker_and_fragment_budget() {
        let mut upload = Upload::new(BlockType::Db, 1, true);
        upload.advance(None).unwrap();
        upload.advance(Some(opening(99999))).unwrap();
        for _ in 0..999 {
            upload
                .advance(Some(ack(vec![0x1e, 1], vec![0, 1, 0, 0xfb, 7])))
                .unwrap();
        }
        assert!(
            upload
                .advance(Some(ack(vec![0x1e, 1], vec![0, 1, 0, 0xfb, 7])))
                .is_err()
        );
        let mut short = Upload::new(BlockType::Db, 1, true);
        short.advance(None).unwrap();
        short.advance(Some(opening(5))).unwrap();
        assert!(
            short
                .advance(Some(ack(vec![0x1e, 0], vec![0, 1, 0, 0xfb, 7])))
                .is_err()
        );
        let mut empty = Upload::new(BlockType::Db, 1, false);
        empty.advance(None).unwrap();
        assert!(empty.advance(Some(opening(0))).is_err());
    }

    /// Even an empty input has one final slice, and oversized transfers fail before offering.
    #[test]
    fn download_empty_body_ordering_and_preflight_budget() {
        assert!(Download::new(&vec![0; 110001], 9, 128).is_err());
        let mut download = Download::new(&[], 9, 128).unwrap();
        assert!(matches!(
            download.advance(None).unwrap(),
            Action::Exchange(_)
        ));
        assert!(matches!(
            download.advance(Some(ack(vec![0x1a], vec![]))).unwrap(),
            Action::Receive
        ));
        let mut parameters = vec![0x1b, 0, 0, 0, 0, 0, 0, 0, 9];
        parameters.extend_from_slice(b"_0A00009P");
        match download
            .advance(Some(Pdu::job(77, parameters.clone(), vec![])))
            .unwrap()
        {
            Action::Send(reply) => {
                assert_eq!(reply.reference, 77);
                assert_eq!(reply.parameters, [0x1b, 0]);
                assert_eq!(reply.data, [0, 0, 0, 0xfb]);
            }
            _ => panic!("expected empty final slice"),
        }
        assert!(matches!(download.advance(None).unwrap(), Action::Receive));
        parameters[0] = 0x1c;
        assert!(matches!(
            download
                .advance(Some(Pdu::job(78, parameters, vec![])))
                .unwrap(),
            Action::Send(_)
        ));
        assert!(matches!(download.advance(None).unwrap(), Action::Done));
    }

    /// Empty nonfinal USERDATA fragments cannot keep a catalog transaction alive.
    #[test]
    fn catalog_rejects_empty_continuation() {
        let mut catalog =
            Catalog::new(crate::protocol::userdata_request(0, 3, 2, None).unwrap(), 2);
        catalog.advance(None).unwrap();
        let reply = Pdu {
            kind: PduType::UserData,
            reference: 1,
            error: 0,
            parameters: vec![0, 1, 0x12, 8, 0x12, 0x83, 2, 1, 0, 1, 0, 0],
            data: vec![0xff, 9, 0, 0],
        };
        assert!(catalog.advance(Some(reply)).is_err());
    }
}
