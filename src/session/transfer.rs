//! Stateful block transfer driver; errors and unwinding invalidate the connection.

use super::Session;
use crate::{
    BlockType, Error, Result,
    blocks::{
        BlockInfo,
        transfer::{Action, Catalog, Transfer},
    },
    protocol::{self, Pdu},
};

/// A failed stateful transfer cannot leave a partially active remote session reusable.
struct Guard<'a> {
    session: &'a mut Session,
    committed: bool,
    started: bool,
}
impl Drop for Guard<'_> {
    /// Queue loss under the lock; application hooks run later through Client::observe.
    fn drop(&mut self) {
        if !self.committed && self.started {
            self.session.lost();
        }
    }
}

impl Session {
    /// Execute pure state-machine actions under one deadline and exclusive guard.
    pub(crate) fn transfer<T: Transfer>(&mut self, mut machine: T) -> Result<T::Output> {
        let deadline = self.deadline();
        self.ensure_ready(deadline)?;
        let mut guard = Guard {
            session: self,
            committed: false,
            started: false,
        };
        let result = (|| {
            let mut input = None;
            loop {
                match machine.advance(input.take())? {
                    Action::Exchange(job) => {
                        let result = guard.session.raw_exchange(job);
                        guard.started |= result.is_ok()
                            || result
                                .as_ref()
                                .is_err_and(|e| !crate::blocks::transfer::not_sent(e));
                        input = Some(result?);
                    }
                    Action::Receive => {
                        guard.started = true;
                        let bytes = guard.session.connection.receive_until(deadline)?;
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
                        guard.session.acquire_permit(deadline)?;
                        guard.session.connection.send_until(&bytes, deadline)?;
                    }
                    Action::Done => return Ok(()),
                }
            }
        })();
        match result {
            Ok(()) => {
                guard.committed = true;
                Ok(machine.finish())
            }
            Err(error) => Err(machine.failure(error)),
        }
    }

    /// Decode and correlate remote block information for catalog queries and DB sizing.
    pub(crate) fn block_info(&mut self, kind: BlockType, number: u16) -> Result<BlockInfo> {
        let payload = self.transfer(Catalog::new(
            protocol::blocks::info_request(0, kind, number)?,
            3,
        ))?;
        let info = self.validate(BlockInfo::decode(&payload))?;
        if info.kind != kind || info.number != number {
            return self.validate(Err(Error::Protocol("block-info identity mismatch")));
        }
        Ok(info)
    }
}
