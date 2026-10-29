//! GUI-local actions. Availability is a read-only query; execution rechecks it.
use super::App;
use crate::i18n::Tr;
use opclook_contract::actions::ActionId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAction {
    OpenConnectDialog,
}

impl UiAction {
    pub fn id(self) -> ActionId {
        match self {
            Self::OpenConnectDialog => ActionId::OpenConnectDialog,
        }
    }
}

impl App {
    /// A translation key rather than rendered text keeps this independent of UI.
    pub fn ui_action_unavailable(&self, action: UiAction) -> Option<Tr> {
        self.ui_action_unavailable_with_palette(action, false)
    }

    pub fn ui_action_unavailable_in_palette(&self, action: UiAction) -> Option<Tr> {
        self.ui_action_unavailable_with_palette(action, true)
    }

    fn ui_action_unavailable_with_palette(
        &self,
        action: UiAction,
        palette_open: bool,
    ) -> Option<Tr> {
        let modal = if palette_open {
            self.dialogs.any_open_except_palette()
        } else {
            self.dialogs.any_open()
        };
        match action {
            UiAction::OpenConnectDialog if modal => Some(Tr::ActionModalOpen),
            UiAction::OpenConnectDialog if self.csv_import.is_some() => {
                Some(Tr::ActionImportInProgress)
            }
            UiAction::OpenConnectDialog => None,
        }
    }

    pub fn execute_ui_action(&mut self, action: UiAction) -> Result<(), Tr> {
        if let Some(reason) = self.ui_action_unavailable(action) {
            return Err(reason);
        }
        match action {
            UiAction::OpenConnectDialog => self.open_connect_dialog(),
        }
        Ok(())
    }
}

pub(super) fn action_unavailable_tr(reason: opclook_engine::ActionUnavailable) -> Tr {
    match reason {
        opclook_engine::ActionUnavailable::RequestIdsExhausted => Tr::RequestIdsExhausted,
        opclook_engine::ActionUnavailable::SessionMissing => Tr::ActionSessionMissing,
        opclook_engine::ActionUnavailable::SessionNotConnected => Tr::ActionSessionNotConnected,
        opclook_engine::ActionUnavailable::ProtocolMismatch => Tr::ActionProtocolMismatch,
    }
}

impl App {
    pub fn write_unavailable(
        &self,
        session: crate::bff::model::SessionId,
        protocol: crate::bff::model::Protocol,
    ) -> Option<Tr> {
        self.engine
            .write_session_unavailable(session, protocol)
            .map(action_unavailable_tr)
    }
}
