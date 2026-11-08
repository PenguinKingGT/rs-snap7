//! Session password operations; plaintext is never stored in client configuration.

use super::AsyncClient;
use crate::{Result, protocol};

impl AsyncClient {
    /// Unlock this session using a one-to-eight-character ASCII password.
    /// The password is encoded transiently and is never retained for automatic replay.
    pub async fn set_session_password(&self, password: &str) -> Result<()> {
        self.observe("set_session_password", async {
            let encoded = protocol::encode_password(password)?;
            self.security(1, Some(&encoded)).await
        })
        .await
    }

    /// Clear the current session password, accepting real-PLC null acknowledgements.
    pub async fn clear_session_password(&self) -> Result<()> {
        self.observe("clear_session_password", async {
            self.security(2, None).await
        })
        .await
    }

    /// Validate both service identity and error layers on security acknowledgements.
    async fn security(&self, subfunction: u8, payload: Option<&[u8]>) -> Result<()> {
        let mut session = self.session().await;
        session
            .transact_checked(
                |reference| protocol::userdata_request(reference, 5, subfunction, payload),
                |reply| protocol::check_userdata_ack(reply, 5, subfunction),
            )
            .await
    }
}
