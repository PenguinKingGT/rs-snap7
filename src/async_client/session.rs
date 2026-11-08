//! Async transaction state with cancellation-safe socket ownership.

mod transfer;

use super::transport::AsyncConnection;
use crate::{
    ClientConfig, Error, Result,
    protocol::{self, Negotiated, Pdu},
};
use std::time::Duration;
use tokio::time::Instant;

/// Exclusive session state shared by async client clones.
#[derive(Debug)]
pub(super) struct Session {
    connection: Option<AsyncConnection>,
    pub(super) limits: Negotiated,
    reference: u16,
    read_plan: Option<std::sync::Arc<crate::planner::ReadPlan>>,
    timeout: Duration,
    operation_deadline: Instant,
    limiter: crate::policy::Limiter,
    origin: Instant,
    config: ClientConfig,
    manual_closed: bool,
    loss_notified: bool,
    events: Vec<crate::policy::ConnectionEvent>,
}

impl Session {
    /// Complete all negotiations before exposing a usable async client.
    pub(super) async fn connect(config: &ClientConfig) -> Result<Self> {
        let deadline = Instant::now() + config.transport.timeout;
        let connection = AsyncConnection::connect(&config.transport, deadline).await?;
        let mut session = Self {
            connection: Some(connection),
            limits: Negotiated {
                caller_jobs: 1,
                callee_jobs: 1,
                pdu_length: config.requested_pdu,
            },
            reference: 0,
            read_plan: None,
            timeout: config.transport.timeout,
            operation_deadline: deadline,
            limiter: crate::policy::Limiter::new(config.rate_limit)?,
            origin: Instant::now(),
            config: config.clone(),
            manual_closed: false,
            loss_notified: false,
            events: Vec::new(),
        };
        let reply = session
            .exchange_until(
                |reference| protocol::setup_request(reference, config.requested_pdu),
                deadline,
            )
            .await?;
        session.limits = protocol::parse_setup(&reply, config.requested_pdu)?;
        Ok(session)
    }
    /// Reuse a pure read plan only when both layout and negotiated limits match.
    pub(super) fn read_plan(
        &mut self,
        inputs: &[crate::Address],
    ) -> Result<std::sync::Arc<crate::planner::ReadPlan>> {
        if let Some(plan) = &self.read_plan
            && plan.matches(inputs, self.limits.pdu_length)
        {
            return Ok(std::sync::Arc::clone(plan));
        }
        let plan = std::sync::Arc::new(crate::planner::ReadPlan::new(
            inputs,
            self.limits.pdu_length,
        )?);
        self.read_plan = Some(std::sync::Arc::clone(&plan));
        Ok(plan)
    }

    /// Whether no prior I/O failure or cancelled exchange invalidated the socket.
    pub(super) fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    /// Total deadline shared by all steps of a logical operation.
    pub(super) fn deadline(&self) -> Instant {
        self.operation_deadline
    }
    /// Drop socket ownership before awaiting, so cancellation cannot leave a live session.
    pub(super) async fn disconnect(&mut self) -> Result<()> {
        self.manual_closed = true;
        let deadline = self.deadline();
        if let Some(connection) = self.connection.take() {
            connection.disconnect(deadline).await?;
        }
        Ok(())
    }
    /// Exchange an advanced raw job without automatic replay.
    pub(super) async fn raw_exchange(&mut self, job: Pdu) -> Result<Pdu> {
        self.exchange_until(|reference| Ok(Pdu { reference, ..job }), self.deadline())
            .await
    }

    /// Start one logical operation after acquiring its exclusive session guard.
    pub(super) fn start_operation(&mut self) {
        self.operation_deadline = Instant::now() + self.timeout;
    }

    /// Execute one job with a new deadline.
    pub(super) async fn transact(&mut self, build: impl FnOnce(u16) -> Result<Pdu>) -> Result<Pdu> {
        self.transact_until(build, self.deadline()).await
    }
    /// Classify a mutation only after its complete service acknowledgement is checked.
    /// Already-classified exchange errors and definite peer refusals are preserved.
    pub(super) async fn transact_checked<T>(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        check: impl FnOnce(&Pdu) -> Result<T>,
    ) -> Result<T> {
        let job = build(0)?;
        let result = self
            .transact(|reference| {
                Ok(Pdu {
                    reference,
                    ..job.clone()
                })
            })
            .await
            .and_then(|response| self.validate(check(&response)));
        result.map_err(|error| crate::policy::mutation_failure(&job, error))
    }

    /// Own the connection locally across awaits. If this future is dropped after
    /// transmission starts, the descriptor closes and the session remains disconnected.
    async fn exchange_until(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        deadline: Instant,
    ) -> Result<Pdu> {
        if !self.is_connected() {
            return Err(Error::Disconnected);
        }
        if Instant::now() >= deadline {
            return Err(Error::DeadlineBeforeSend);
        }
        let next = self.reference.wrapping_add(1);
        let job = build(next)?;
        if protocol::read_response_len(&job)
            .is_some_and(|size| size > usize::from(self.limits.pdu_length))
        {
            return Err(Error::InvalidInput("read response exceeds negotiated PDU"));
        }
        let bytes = job.encode()?;
        if bytes.len() > usize::from(self.limits.pdu_length) {
            return Err(Error::InvalidInput("request exceeds negotiated PDU"));
        }
        self.acquire_permit(deadline).await?;
        self.reference = next;
        let mut connection = self.connection.take().ok_or(Error::Disconnected)?;
        let result = async {
            connection.send(&bytes, deadline).await?;
            for _ in 0..=3 {
                let bytes = connection.receive(deadline).await?;
                if bytes.len() > usize::from(self.limits.pdu_length) {
                    return Err(Error::Protocol("response exceeds negotiated PDU"));
                }
                let reply = Pdu::decode(&bytes)?;
                match protocol::validate_reference(next, reply.reference) {
                    Err(Error::StaleReference { .. }) => continue,
                    other => other?,
                }
                reply.check_response()?;
                return Ok(reply);
            }
            Err(Error::Protocol("stale response retry limit reached"))
        }
        .await;
        if result.is_err()
            && !matches!(result, Err(Error::Plc(_) | Error::Authentication(_)))
            && !self.loss_notified
        {
            self.events
                .push(crate::policy::ConnectionEvent::Disconnected);
            self.loss_notified = true;
        }
        if result.is_ok() || matches!(result, Err(Error::Plc(_) | Error::Authentication(_))) {
            self.connection = Some(connection);
        }
        result
    }
    /// A malformed service reply also invalidates its completed protocol stream.
    pub(super) fn validate<T>(&mut self, result: Result<T>) -> Result<T> {
        if matches!(result, Err(Error::Protocol(_))) {
            if self.connection.is_some() && !self.loss_notified {
                self.events
                    .push(crate::policy::ConnectionEvent::Disconnected);
                self.loss_notified = true;
            }
            self.connection = None;
        }
        result
    }
    /// Apply recovery only to known-safe initial reads, never replaying writes.
    pub(super) async fn transact_until(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        deadline: Instant,
    ) -> Result<Pdu> {
        let job = build(0)?;
        if !self.is_connected() {
            if job.kind == protocol::PduType::UserData && job.parameters.len() > 8 {
                return Err(Error::Disconnected);
            }
            self.ensure_ready(deadline).await?;
        }
        let result = self
            .exchange_until(
                |reference| {
                    Ok(Pdu {
                        reference,
                        ..job.clone()
                    })
                },
                deadline,
            )
            .await;
        if result.as_ref().is_err_and(crate::policy::recoverable)
            && self.config.recovery.enabled
            && !self.manual_closed
            && crate::policy::replay_safe(&job)
        {
            Box::pin(self.recover(deadline)).await?;
            return self
                .exchange_until(|reference| Ok(Pdu { reference, ..job }), deadline)
                .await;
        }
        result.map_err(|error| crate::policy::mutation_failure(&job, error))
    }

    /// Reconnect before a new request only when no request has yet been sent.
    pub(super) async fn ensure_ready(&mut self, deadline: Instant) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }
        if !self.manual_closed && !self.loss_notified {
            self.events
                .push(crate::policy::ConnectionEvent::Disconnected);
            self.loss_notified = true;
        }
        if self.config.recovery.enabled && !self.manual_closed {
            Box::pin(self.recover(deadline)).await
        } else {
            Err(Error::Disconnected)
        }
    }

    /// Attempt fresh negotiations within the original operation deadline.
    async fn recover(&mut self, deadline: Instant) -> Result<()> {
        let mut last = Error::Disconnected;
        for attempt in 0..self.config.recovery.max_retries {
            let delay = self
                .config
                .recovery
                .delay(attempt, crate::policy::jitter_sample());
            if Instant::now() >= deadline
                || delay >= deadline.saturating_duration_since(Instant::now())
            {
                return Err(Error::DeadlineBeforeSend);
            }
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match self.reopen(deadline).await {
                Ok(()) => {
                    self.loss_notified = false;
                    self.events
                        .push(crate::policy::ConnectionEvent::Reconnected);
                    return Ok(());
                }
                Err(error) if crate::policy::recoverable(&error) => last = error,
                Err(error) => return Err(error),
            }
        }
        Err(last)
    }

    /// Replace transport and reset references/plan state, retaining limiter credit.
    async fn reopen(&mut self, deadline: Instant) -> Result<()> {
        self.connection = Some(AsyncConnection::connect(&self.config.transport, deadline).await?);
        self.reference = 0;
        self.read_plan = None;
        let requested = self.config.requested_pdu;
        self.limits.pdu_length = requested;
        let result = self
            .exchange_until(
                |reference| protocol::setup_request(reference, requested),
                deadline,
            )
            .await
            .and_then(|reply| protocol::parse_setup(&reply, requested));
        match result {
            Ok(limits) => {
                self.limits = limits;
                Ok(())
            }
            Err(error) => {
                self.connection = None;
                Err(error)
            }
        }
    }

    /// Drain transitions while locked; callers dispatch them after unlocking.
    pub(super) fn take_events(&mut self) -> Vec<crate::policy::ConnectionEvent> {
        std::mem::take(&mut self.events)
    }

    /// Manual disconnect prevents idle workers from silently reconnecting.
    pub(super) fn manually_closed(&self) -> bool {
        self.manual_closed
    }

    /// Probe liveness with a standard read-only CPU-status SZL request.
    pub(super) async fn heartbeat(&mut self) -> Result<()> {
        let response = self
            .transact(|reference| {
                protocol::userdata_request(reference, 4, 1, Some(&[4, 0x24, 0, 0]))
            })
            .await?;
        let reply = self.validate(protocol::parse_userdata(&response, 4, 1, false))?;
        if reply.payload.len() < 4 || reply.payload[..4] != [4, 0x24, 0, 0] {
            return self.validate(Err(Error::Protocol("heartbeat SZL identity mismatch")));
        }
        Ok(())
    }
    /// Charge one wire send against the current session's limiter and total deadline.
    async fn acquire_permit(&mut self, deadline: Instant) -> Result<()> {
        loop {
            if Instant::now() >= deadline {
                return Err(Error::DeadlineBeforeSend);
            }
            let wait = self.limiter.poll(self.origin.elapsed())?;
            if wait.is_zero() {
                break;
            }
            if wait >= deadline.saturating_duration_since(Instant::now()) {
                return Err(Error::DeadlineBeforeSend);
            }
            tokio::time::sleep(wait).await;
        }
        Ok(())
    }
}
