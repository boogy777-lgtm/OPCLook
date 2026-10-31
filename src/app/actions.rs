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
        opclook_engine::ActionUnavailable::ReadPending => Tr::ActionReadPending,
        opclook_engine::ActionUnavailable::SessionNotConnected => Tr::ActionSessionNotConnected,
        opclook_engine::ActionUnavailable::ProtocolMismatch => Tr::ActionProtocolMismatch,
    }
}

impl App {
    pub fn write_dialog_unavailable(
        &self,
        session: crate::bff::model::SessionId,
        protocol: crate::bff::model::Protocol,
    ) -> Option<Tr> {
        if self.dialogs.write.epoch.is_none()
            || self.dialogs.write.epoch != self.engine.connection_id(session)
        {
            return Some(Tr::PaletteWriteStale);
        }
        self.write_unavailable(session, protocol)
    }
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

impl App {
    /// Open a fresh palette without dispatching any command.
    pub fn open_command_palette(&mut self) {
        self.dialogs.palette = Default::default();
        self.dialogs.palette.open = true;
        self.dialogs.palette.focus_pending = true;
        self.prepare_palette_context();
    }

    /// Capture once; the first-frame fallback supports directly opened test dialogs.
    pub fn prepare_palette_context(&mut self) {
        if self.dialogs.palette.context_initialized {
            return;
        }
        let session = self.active_session;
        self.dialogs.palette.context_session = session;
        self.dialogs.palette.context_epoch = session.and_then(|id| self.engine.connection_id(id));
        self.dialogs.palette.context_initialized = true;
    }

    pub fn palette_unavailable(&self, id: ActionId) -> Option<Tr> {
        use crate::bff::model::NodeClassFilter;
        use opclook_contract::actions::DomainAction;
        if id == ActionId::OpenConnectDialog {
            return self.ui_action_unavailable_in_palette(UiAction::OpenConnectDialog);
        }
        if self.dialogs.any_open_except_palette() {
            return Some(Tr::ActionModalOpen);
        }
        if self.csv_import.is_some() {
            return Some(Tr::ActionImportInProgress);
        }
        let Some(session) = self.dialogs.palette.context_session else {
            return Some(Tr::PaletteNeedsContext);
        };
        if self.active_session != Some(session)
            || self.dialogs.palette.context_epoch.is_none()
            || self.engine.connection_id(session) != self.dialogs.palette.context_epoch
        {
            return Some(Tr::PaletteConnectionChanged);
        }
        let Some(view) = self.session(session) else {
            return Some(Tr::ActionSessionMissing);
        };
        let protocol = view.protocol();
        match id {
            ActionId::Browse => {
                let node_id = &self.dialogs.palette.browse_node;
                if node_id.len() > 16 * 1024
                    || node_id == crate::bff::model::BrowseNode::VIEWS_ROOT_ID
                {
                    return Some(Tr::PaletteBrowseUnknown);
                }
                self.engine
                    .action_unavailable(&DomainAction::Browse {
                        session,
                        protocol,
                        node_id: node_id.clone(),
                        filter: NodeClassFilter::All,
                        view_id: None,
                    })
                    .map(action_unavailable_tr)
            }
            ActionId::Write => {
                if self.selected_rows.len() != 1 {
                    return Some(Tr::PaletteSelectOne);
                }
                let Some(key) = self.selected_rows.iter().next() else {
                    return Some(Tr::PaletteSelectOne);
                };
                let Some(row) = self
                    .rows
                    .iter()
                    .find(|row| row.key == *key && row.session == session)
                else {
                    return Some(Tr::PaletteSelectOne);
                };
                if !row.writable {
                    return Some(Tr::ReadOnly);
                }
                self.write_unavailable(session, protocol)
            }
            ActionId::OpenConnectDialog => None,
        }
    }

    /// Rechecks the captured connection and hands off to existing domain UI.
    pub fn execute_palette_action(&mut self, id: ActionId) -> Result<(), Tr> {
        if let Some(reason) = self.palette_unavailable(id) {
            return Err(reason);
        }
        match id {
            ActionId::OpenConnectDialog => {
                self.dialogs.palette.open = false;
                self.execute_ui_action(UiAction::OpenConnectDialog)
            }
            ActionId::Browse => {
                let Some(session) = self.dialogs.palette.context_session else {
                    return Err(Tr::PaletteNeedsContext);
                };
                let node = self.dialogs.palette.browse_node.clone();
                let Some(view) = self.session(session) else {
                    return Err(Tr::ActionSessionMissing);
                };
                let known =
                    node.is_empty() || view.tree().iter().any(|item| item.node.node_id == node);
                let protocol = view.protocol();
                let (filter, view_id) = view
                    .ua()
                    .map_or((crate::bff::model::NodeClassFilter::All, None), |ua| {
                        (ua.ua_browse_filter, ua.ua_view_id.clone())
                    });
                self.dialogs.palette.open = false;
                if known {
                    self.browse(session, node);
                } else {
                    self.send_with(
                        session,
                        protocol,
                        crate::bff::model::CommandKind::Browse {
                            node_id: node.clone(),
                            filter,
                            view_id,
                        },
                        super::PendingCtx::PaletteBrowse(node),
                    );
                }
                Ok(())
            }
            ActionId::Write => {
                let Some(key) = self.selected_rows.iter().next().copied() else {
                    return Err(Tr::PaletteSelectOne);
                };
                self.dialogs.palette.open = false;
                self.open_write_dialog(vec![key]);
                Ok(())
            }
        }
    }
}
