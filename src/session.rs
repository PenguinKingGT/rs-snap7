//! Exclusive session state and correlated blocking transaction execution.

mod transfer;

use crate::client::ClientConfig;
use crate::protocol::{self, Negotiated, Pdu};
use crate::transport::Connection;
use crate::{Error, Result};
use std::time::{Duration, Instant};

/// A transport owner and negotiated S7 transaction state, guarded by the client.
#[derive(Debug)]
pub(crate) struct Session {
    connection: Connection,
    /// Negotiated limits used by operation and packet planners.
    pub(crate) limits: Negotiated,
    reference: u16,
    read_plan: Option<std::sync::Arc<crate::planner::ReadPlan>>,
    timeout: Duration,
    operation_deadline: Instant,
    limiter: crate::policy::Limiter,
    origin: Instant,
    config: ClientConfig,
    manual_closed: bool,
    events: Vec<crate::policy::ConnectionEvent>,
}

impl Session {
    /// Establish all layers before publishing this session to a client.
    pub(crate) fn connect(config: &ClientConfig) -> Result<Self> {
        let deadline = Instant::now() + config.transport.timeout;
        let connection = Connection::connect(&config.transport)?;
        let mut session = Self {
            connection,
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
            events: Vec::new(),
        };
        let reply = session.exchange_until(
            |reference| protocol::setup_request(reference, config.requested_pdu),
            session.deadline(),
        )?;
        session.limits = protocol::parse_setup(&reply, config.requested_pdu)?;
        Ok(session)
    }

    /// Reuse a pure read plan only when both layout and negotiated limits match.
    pub(crate) fn read_plan(
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

    /// Whether transport has survived every prior exchange.
    pub(crate) fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }
    /// Gracefully disconnect the owned transport.
    pub(crate) fn disconnect(&mut self) -> Result<()> {
        self.manual_closed = true;
        self.connection.disconnect()
    }

    /// Exchange an advanced raw job without automatic replay.
    pub(crate) fn raw_exchange(&mut self, job: Pdu) -> Result<Pdu> {
        self.exchange_until(|reference| Ok(Pdu { reference, ..job }), self.deadline())
    }

    /// Start one logical operation after acquiring its exclusive session guard.
    pub(crate) fn start_operation(&mut self) {
        self.operation_deadline = Instant::now() + self.timeout;
    }

    /// Reserve a reference, encode, exchange, and validate without replaying requests.
    pub(crate) fn transact(&mut self, build: impl FnOnce(u16) -> Result<Pdu>) -> Result<Pdu> {
        self.transact_until(build, self.deadline())
    }

    /// Classify a mutation only after its complete service acknowledgement is checked.
    /// Already-classified exchange errors and definite peer refusals are preserved.
    pub(crate) fn transact_checked<T>(
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
            .and_then(|response| self.validate(check(&response)));
        result.map_err(|error| crate::policy::mutation_failure(&job, error))
    }

    /// Create a total operation deadline shared by every continuation exchange.
    pub(crate) fn deadline(&self) -> Instant {
        self.operation_deadline
    }

    /// Execute one exchange within an existing operation deadline.
    fn exchange_until(
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
        let request = job.encode()?;
        if request.len() > usize::from(self.limits.pdu_length) {
            return Err(Error::InvalidInput("request exceeds negotiated PDU"));
        }
        self.acquire_permit(deadline)?;
        self.reference = next;
        if let Err(error) = self.connection.send_until(&request, deadline) {
            self.lost();
            return Err(error);
        }
        let result = self.receive_matching(deadline);
        if result
            .as_ref()
            .is_err_and(|e| !matches!(e, Error::Plc(_) | Error::Authentication(_)))
        {
            self.lost();
        }
        result
    }

    /// Invalidate a structurally wrong service reply before another job reuses it.
    pub(crate) fn validate<T>(&mut self, result: Result<T>) -> Result<T> {
        if matches!(result, Err(Error::Protocol(_))) {
            self.lost();
        }
        result
    }

    /// Ignore a bounded number of stale frames under the original deadline.
    fn receive_matching(&mut self, deadline: Instant) -> Result<Pdu> {
        for _ in 0..=3 {
            let bytes = self.connection.receive_until(deadline)?;
            if bytes.len() > usize::from(self.limits.pdu_length) {
                return Err(Error::Protocol("response exceeds negotiated PDU"));
            }
            let response = Pdu::decode(&bytes)?;
            match protocol::validate_reference(self.reference, response.reference) {
                Err(Error::StaleReference { .. }) => continue,
                other => other?,
            }
            response.check_response()?;
            return Ok(response);
        }
        Err(Error::Protocol("stale response retry limit reached"))
    }
    /// Apply recovery only to known-safe initial reads, never replaying writes.
    pub(crate) fn transact_until(
        &mut self,
        build: impl FnOnce(u16) -> Result<Pdu>,
        deadline: Instant,
    ) -> Result<Pdu> {
        let job = build(0)?;
        if !self.is_connected() {
            if job.kind == protocol::PduType::UserData && job.parameters.len() > 8 {
                return Err(Error::Disconnected);
            }
            self.ensure_ready(deadline)?;
        }
        let result = self.exchange_until(
            |reference| {
                Ok(Pdu {
                    reference,
                    ..job.clone()
                })
            },
            deadline,
        );
        if result.as_ref().is_err_and(crate::policy::recoverable)
            && self.config.recovery.enabled
            && !self.manual_closed
            && crate::policy::replay_safe(&job)
        {
            self.recover(deadline)?;
            return self.exchange_until(|reference| Ok(Pdu { reference, ..job }), deadline);
        }
        result.map_err(|error| crate::policy::mutation_failure(&job, error))
    }

    /// Reconnect before a new request only when no request has yet been sent.
    pub(crate) fn ensure_ready(&mut self, deadline: Instant) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }
        if self.config.recovery.enabled && !self.manual_closed {
            self.recover(deadline)
        } else {
            Err(Error::Disconnected)
        }
    }

    /// Attempt fresh negotiations within the original operation deadline.
    fn recover(&mut self, deadline: Instant) -> Result<()> {
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
                std::thread::sleep(delay);
            }
            match self.reopen(deadline) {
                Ok(()) => {
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
    fn reopen(&mut self, deadline: Instant) -> Result<()> {
        let mut transport = self.config.transport.clone();
        transport.timeout = deadline.saturating_duration_since(Instant::now());
        self.connection = Connection::connect(&transport)?;
        self.reference = 0;
        self.read_plan = None;
        let requested = self.config.requested_pdu;
        self.limits.pdu_length = requested;
        let result = self
            .exchange_until(
                |reference| protocol::setup_request(reference, requested),
                deadline,
            )
            .and_then(|reply| protocol::parse_setup(&reply, requested));
        match result {
            Ok(limits) => {
                self.limits = limits;
                Ok(())
            }
            Err(error) => {
                self.lost();
                Err(error)
            }
        }
    }

    /// Drain transitions while locked; callers dispatch them after unlocking.
    pub(crate) fn take_events(&mut self) -> Vec<crate::policy::ConnectionEvent> {
        std::mem::take(&mut self.events)
    }

    /// Manual disconnect prevents idle workers from silently reconnecting.
    pub(crate) fn manually_closed(&self) -> bool {
        self.manual_closed
    }

    /// Probe liveness with a standard read-only CPU-status SZL request.
    pub(crate) fn heartbeat(&mut self) -> Result<()> {
        let response = self.transact(|reference| {
            protocol::userdata_request(reference, 4, 1, Some(&[4, 0x24, 0, 0]))
        })?;
        let reply = self.validate(protocol::parse_userdata(&response, 4, 1, false))?;
        if reply.payload.len() < 4 || reply.payload[..4] != [4, 0x24, 0, 0] {
            return self.validate(Err(Error::Protocol("heartbeat SZL identity mismatch")));
        }
        Ok(())
    }

    /// Queue a loss transition exactly once before invalidating a live transport.
    fn lost(&mut self) {
        if self.connection.is_connected() {
            self.events
                .push(crate::policy::ConnectionEvent::Disconnected);
        }
        self.connection.abort();
    }
    /// Charge one wire send against the current session's limiter and total deadline.
    fn acquire_permit(&mut self, deadline: Instant) -> Result<()> {
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
            std::thread::sleep(wait);
        }
        Ok(())
    }
}
