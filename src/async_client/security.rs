//! Session password operations; plaintext is never stored in client configuration.

use super::AsyncClient;
use crate::{Error, Result, protocol};

impl AsyncClient {
    /// Unlock this session using a one-to-eight-character ASCII password.
    /// The password is encoded transiently and is never retained for automatic replay.
    pub async fn set_session_password(&self, password: &str) -> Result<()> {
        let encoded = protocol::encode_password(password)?;
        self.security(1, Some(&encoded)).await
    }

    /// Clear the current session password, accepting real-PLC null acknowledgements.
    pub async fn clear_session_password(&self) -> Result<()> {
        self.security(2, None).await
    }

    /// Validate both service identity and error layers on security acknowledgements.
    async fn security(&self, subfunction: u8, payload: Option<&[u8]>) -> Result<()> {
        let mut session = self.session().await;
        let response = session
            .transact(|reference| protocol::userdata_request(reference, 5, subfunction, payload))
            .await?;
        let reply = session.validate(protocol::parse_userdata(&response, 5, subfunction, true))?;
        if reply.more || !reply.payload.is_empty() {
            return session.validate(Err(Error::Protocol(
                "unexpected security acknowledgement payload",
            )));
        }
        Ok(())
    }
}
