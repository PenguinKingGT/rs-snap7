//! Session password operations; plaintext is never stored in client configuration.

use super::Client;
use crate::{Result, protocol};

impl Client {
    /// Unlock this session using a one-to-eight-character ASCII password.
    /// The password is encoded transiently and is never retained for automatic replay.
    pub fn set_session_password(&self, password: &str) -> Result<()> {
        self.observe("set_session_password", || {
            let encoded = protocol::encode_password(password)?;
            self.security(1, Some(&encoded))
        })
    }

    /// Clear the current session password, accepting real-PLC null acknowledgements.
    pub fn clear_session_password(&self) -> Result<()> {
        self.observe("clear_session_password", || self.security(2, None))
    }

    /// Validate both service identity and error layers on security acknowledgements.
    fn security(&self, subfunction: u8, payload: Option<&[u8]>) -> Result<()> {
        let mut session = self.session()?;
        session.transact_checked(
            |reference| protocol::userdata_request(reference, 5, subfunction, payload),
            |reply| protocol::check_userdata_ack(reply, 5, subfunction),
        )
    }
}
