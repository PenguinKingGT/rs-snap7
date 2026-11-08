//! Bounded SZL collection and typed diagnostic operations.

use super::AsyncClient;
use crate::{
    CpuState, Error, Result, protocol,
    szl::{self, CpInfo, CpuInfo, DiagnosticEntry, OrderCode, Protection, Szl},
};

impl AsyncClient {
    /// Read a complete system status list, preserving its unmodified list body.
    /// ID/index, fragment count, total payload (1 MiB), and operation deadline are checked.
    pub async fn read_szl(&self, id: u16, index: u16) -> Result<Szl> {
        self.observer
            .run_async("read_szl", async {
                let mut session = self.session().await;
                let deadline = session.deadline();
                let mut payload = Vec::from(id.to_be_bytes());
                payload.extend_from_slice(&index.to_be_bytes());
                let response = session
                    .transact_until(
                        |reference| protocol::userdata_request(reference, 4, 1, Some(&payload)),
                        deadline,
                    )
                    .await?;
                let mut fragment =
                    session.validate(protocol::parse_userdata(&response, 4, 1, false))?;
                if fragment.payload.len() < 4 || fragment.payload[..4] != payload {
                    return session.validate(Err(Error::Protocol("SZL identity mismatch")));
                }
                let mut data = fragment.payload[4..].to_vec();
                for _ in 0..100 {
                    if !fragment.more {
                        return Ok(Szl { id, index, data });
                    }
                    let response = session
                        .transact_until(
                            |reference| {
                                Ok(protocol::userdata_followup(
                                    reference,
                                    4,
                                    1,
                                    fragment.sequence,
                                ))
                            },
                            deadline,
                        )
                        .await?;
                    fragment =
                        session.validate(protocol::parse_userdata(&response, 4, 1, false))?;
                    if fragment.payload.is_empty() && fragment.more
                        || data.len() + fragment.payload.len() > 1024 * 1024
                    {
                        return session
                            .validate(Err(Error::Protocol("SZL continuation exceeds limits")));
                    }
                    data.extend_from_slice(&fragment.payload);
                }
                if fragment.more {
                    return session.validate(Err(Error::Protocol("SZL fragment limit reached")));
                }
                Ok(Szl { id, index, data })
            })
            .await
    }

    /// Read the available SZL ID list as the upstream raw list body.
    pub async fn read_szl_list(&self) -> Result<Vec<u8>> {
        self.observer
            .run_async("read_szl_list", async {
                Ok(self.read_szl(0, 0).await?.data)
            })
            .await
    }
    /// Read CPU component identification.
    pub async fn get_cpu_info(&self) -> Result<CpuInfo> {
        self.observer
            .run_async("get_cpu_info", async {
                szl::parse_cpu_info(&self.read_szl(0x001c, 0).await?.data)
            })
            .await
    }
    /// Read actual CPU state through SZL 0x0424, unlike upstream's constant RUN stub.
    pub async fn get_cpu_state(&self) -> Result<CpuState> {
        self.observer
            .run_async("get_cpu_state", async {
                szl::parse_cpu_state(&self.read_szl(0x0424, 0).await?.data)
            })
            .await
    }
    /// Read communication processor information using the upstream field layout.
    pub async fn get_cp_info(&self) -> Result<CpInfo> {
        self.observer
            .run_async("get_cp_info", async {
                szl::parse_cp_info(&self.read_szl(0x0131, 0).await?.data)
            })
            .await
    }
    /// Read module order code, preferring installed firmware over bootloader version.
    pub async fn get_order_code(&self) -> Result<OrderCode> {
        self.observer
            .run_async("get_order_code", async {
                szl::parse_order_code(&self.read_szl(0x0011, 0).await?)
            })
            .await
    }
    /// Read the CPU protection fields.
    pub async fn get_protection(&self) -> Result<Protection> {
        self.observer
            .run_async("get_protection", async {
                szl::parse_protection(&self.read_szl(0x0232, 0).await?.data)
            })
            .await
    }
    /// Read diagnostic events in wire order (upstream returns newest first).
    pub async fn read_diagnostic_buffer(&self) -> Result<Vec<DiagnosticEntry>> {
        self.observer
            .run_async("read_diagnostic_buffer", async {
                szl::parse_diagnostic_buffer(&self.read_szl(0x00a0, 0).await?.data)
            })
            .await
    }
}
