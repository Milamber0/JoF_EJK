//! Communication remains attached to the remote server during map preparation.
use crate::GpuState;
use sjk_client::ClientSession;

impl GpuState {
    pub(crate) fn communication_session_mut(&mut self) -> Option<&mut ClientSession> {
        self.resident
            .session
            .as_mut()
            .or(self.live_session.as_mut())
    }

    pub(super) fn consume_resident_messages(&mut self) {
        let Some(session) = &mut self.resident.session else {
            return;
        };
        crate::server_commands::consume_messages(
            session,
            &self.localization,
            &mut self.chat,
            self.console.as_mut(),
        );
    }
}
