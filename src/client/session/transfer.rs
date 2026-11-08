//! Async state-machine driver with whole-transfer cancellation invalidation.

use super::Session;
use crate::{
    BlockType, Error, Result,
    blocks::{
        BlockInfo,
        transfer::{Action, Catalog, Transfer},
    },
    protocol::{self, Pdu},
};

/// Even cancellation between wire exchanges closes an active upload/download session.
struct Guard<'a> {
    session: &'a mut Session,
    committed: bool,
    started: bool,
}
impl Drop for Guard<'_> {
    /// The future borrowing the connection is dropped before this guard runs.
    fn drop(&mut self) {
        if !self.committed && self.started {
            self.session.connection = None;
            if !self.session.loss_notified {
                self.session
                    .events
                    .push(crate::policy::ConnectionEvent::Disconnected);
                self.session.loss_notified = true;
            }
        }
    }
}

impl Session {
    /// Drive the pure transfer machine once, without replay after transmission.
    pub(in crate::client) async fn transfer<T: Transfer>(
        &mut self,
        mut machine: T,
    ) -> Result<T::Output> {
        let deadline = self.deadline();
        self.ensure_ready(deadline).await?;
        let mut guard = Guard {
            session: self,
            committed: false,
            started: false,
        };
        let result = async {
            let mut input = None;
            loop {
                match machine.advance(input.take())? {
                    Action::Exchange(job) => {
                        let result = guard.session.raw_exchange(job).await;
                        guard.started |= result.is_ok()
                            || result
                                .as_ref()
                                .is_err_and(|e| !crate::blocks::transfer::not_sent(e));
                        input = Some(result?);
                    }
                    Action::Receive => {
                        guard.started = true;
                        let bytes = guard
                            .session
                            .connection
                            .as_mut()
                            .ok_or(Error::Disconnected)?
                            .receive(deadline)
                            .await?;
                        if bytes.len() > usize::from(guard.session.limits.pdu_length) {
                            return Err(Error::Protocol("transfer response exceeds PDU"));
                        }
                        let job = Pdu::decode(&bytes)?;
                        if job.error != 0 {
                            return Err(Error::Plc(job.error));
                        }
                        input = Some(job);
                    }
                    Action::Send(reply) => {
                        let bytes = reply.encode()?;
                        if bytes.len() > usize::from(guard.session.limits.pdu_length) {
                            return Err(Error::InvalidInput("transfer reply exceeds PDU"));
                        }
                        guard.session.acquire_permit(deadline).await?;
                        guard
                            .session
                            .connection
                            .as_mut()
                            .ok_or(Error::Disconnected)?
                            .send(&bytes, deadline)
                            .await?;
                    }
                    Action::Done => return Ok(()),
                }
            }
        }
        .await;
        match result {
            Ok(()) => {
                guard.committed = true;
                Ok(machine.finish())
            }
            Err(error) => Err(machine.failure(error)),
        }
    }

    /// Shared metadata interpretation and explicit requested-block correlation.
    pub(in crate::client) async fn block_info(
        &mut self,
        kind: BlockType,
        number: u16,
    ) -> Result<BlockInfo> {
        let payload = self
            .transfer(Catalog::new(
                protocol::blocks::info_request(0, kind, number)?,
                3,
            ))
            .await?;
        let info = self.validate(BlockInfo::decode(&payload))?;
        if info.kind != kind || info.number != number {
            return self.validate(Err(Error::Protocol("block-info identity mismatch")));
        }
        Ok(info)
    }
}
