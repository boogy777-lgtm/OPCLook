use super::*;
use crate::bff::model::{
    CommandKind, ItemKey, ItemSpec, Protocol, SessionId, Value, VarType, WriteRequest,
};
use crate::i18n::{Tr, fmt, tr};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};

impl App {
    pub fn add_items(&mut self, session: SessionId, items: Vec<(String, String)>) -> bool {
        let Some(view) = self.session(session) else {
            return false;
        };
        let protocol = view.protocol();
        let server_label = view.endpoint().display_name();
        let mut specs = Vec::new();
        let mut added = 0usize;
        let mut exhausted = false;
        for (item_id, display_name) in items {
            if item_id.trim().is_empty() {
                continue;
            }
            // Index lookup (A4): a CSV import or an add-children job calls this
            // once per item, so a scan of all rows would be O(N^2).
            if self.has_row_for_item(session, &item_id) {
                continue;
            }
            let Some(key) = self.next_key else {
                exhausted = true;
                let message = tr(self.lang(), Tr::ItemIdsExhausted).to_string();
                self.status.clone_from(&message);
                self.journal.error(message);
                break;
            };
            self.next_key = key.successor();
            let display_name = if display_name.trim().is_empty() {
                item_id.clone()
            } else {
                display_name
            };
            self.push_row(Row {
                key,
                session,
                item_id: item_id.clone(),
                display_name: display_name.clone(),
                server_label: server_label.clone(),
                data_type: None,
                canonical_vt: None,
                write_type: None,
                writable: false,
                value: Value::Empty,
                quality: RowQuality::NoData(tr(self.lang(), Tr::NoData).to_string()),
                timestamp: None,
                description: String::new(),
                pending: true,
                poll_only: false,
                changed_at: None,
                value_text_cache: Default::default(),
                timestamp_text_cache: Default::default(),
            });
            specs.push(ItemSpec { key, item_id });
            added += 1;
        }
        if specs.is_empty() {
            if !exhausted {
                self.journal.info_l(Tr::AddFailedEmpty, &[]);
            }
            return false;
        }
        let keys: Vec<ItemKey> = specs.iter().map(|spec| spec.key).collect();
        if self
            .send_with(
                session,
                protocol,
                CommandKind::AddItems { items: specs },
                PendingCtx::AddKeys(keys.clone()),
            )
            .is_some()
        {
            self.journal.info_l(Tr::AddedSignals, &[&added.to_string()]);
            true
        } else {
            self.remove_rows(&keys.into_iter().collect());
            false
        }
    }

    pub(super) fn register_rows(&mut self, session: SessionId) {
        let Some(view) = self.session(session) else {
            return;
        };
        let protocol = view.protocol();
        let mut specs = Vec::new();
        for row in self.rows.iter_mut().filter(|r| r.session == session) {
            row.pending = true;
            // The new session decides again which items it can push (P1).
            row.poll_only = false;
            specs.push(ItemSpec {
                key: row.key,
                item_id: row.item_id.clone(),
            });
        }
        if !specs.is_empty() {
            let keys: Vec<ItemKey> = specs.iter().map(|spec| spec.key).collect();
            if self
                .send_with(
                    session,
                    protocol,
                    CommandKind::AddItems { items: specs },
                    PendingCtx::AddKeys(keys.clone()),
                )
                .is_none()
            {
                for row in self.rows.iter_mut().filter(|row| keys.contains(&row.key)) {
                    row.pending = false;
                }
            }
        }
    }

    pub fn remove_selected_rows(&mut self) {
        if self.selected_rows.is_empty() {
            return;
        }
        let mut by_session: Vec<(SessionId, Protocol, Vec<ItemKey>)> = Vec::new();
        for row in self
            .rows
            .iter()
            .filter(|r| self.selected_rows.contains(&r.key))
        {
            if let Some(view) = self.sessions.iter().find(|s| s.id == row.session) {
                if let Some(entry) = by_session.iter_mut().find(|e| e.0 == row.session) {
                    entry.2.push(row.key);
                } else {
                    by_session.push((row.session, view.protocol(), vec![row.key]));
                }
            }
        }
        let mut removed = std::collections::HashSet::new();
        for (session, protocol, keys) in by_session {
            if self
                .send(
                    session,
                    protocol,
                    CommandKind::RemoveItems { keys: keys.clone() },
                )
                .is_some()
            {
                removed.extend(keys);
            }
        }
        let count = removed.len();
        self.remove_rows(&removed);
        self.selected_rows.retain(|key| !removed.contains(key));
        self.table_focus = None;
        self.table_anchor = None;
        if count > 0 {
            self.journal
                .info_l(Tr::RemovedSignals, &[&count.to_string()]);
        }
    }
}

impl App {
    pub fn open_write_dialog(&mut self, keys: Vec<ItemKey>) {
        let mut keys: Vec<ItemKey> = if keys.is_empty() {
            self.selected_rows.iter().copied().collect()
        } else {
            keys
        };
        let existing: std::collections::HashSet<_> = self.rows.iter().map(|row| row.key).collect();
        keys.retain(|key| existing.contains(key));
        if keys.is_empty() {
            return;
        }
        let first = self
            .rows
            .iter()
            .find(|r| r.key == keys[0])
            .map(|r| (r.session, r.value.to_string(), r.canonical_vt));
        let Some((first_session, first_text, _first_vt)) = first else {
            return;
        };
        let same_session = self
            .rows
            .iter()
            .filter(|r| keys.contains(&r.key))
            .all(|r| r.session == first_session);
        if !same_session {
            let lang = self.lang();
            self.dialogs.message = Some(MessageDialog {
                title: tr(lang, Tr::ChangeValueTitle).to_string(),
                text: tr(lang, Tr::SameServerMsg).to_string(),
                details: String::new(),
            });
            return;
        }
        self.dialogs.write.keys = keys;
        self.dialogs.write.text = first_text;
        self.dialogs.write.type_index = 0;
        self.dialogs.write.error = None;
        self.dialogs.write.focus_pending = true;
        self.dialogs.write.open = true;
    }

    pub fn apply_write(&mut self) {
        let keys = self.dialogs.write.keys.clone();
        if keys.is_empty() {
            return;
        }
        let request_list = match self.build_write_requests(&keys) {
            Ok(requests) => requests,
            Err(message) => {
                self.dialogs.write.error = Some(message);
                // Select the rejected text again so the next keystroke
                // replaces it (K3).
                self.dialogs.write.focus_pending = true;
                return;
            }
        };
        if request_list.requests.is_empty() {
            return;
        }
        // With confirm_write enabled, keep the dialog open and ask explicitly,
        // mirroring the original message before the write is sent.
        if self.settings.view.options.confirm_write {
            let names: Vec<String> = keys
                .iter()
                .filter_map(|key| self.rows.iter().find(|r| r.key == *key))
                .map(|r| r.display_name.clone())
                .collect();
            let value_text = self.dialogs.write.text.clone();
            let lang = self.lang();
            let text = if names.len() == 1 {
                fmt(lang, Tr::ConfirmWriteOne, &[&names[0], &value_text])
            } else {
                fmt(
                    lang,
                    Tr::ConfirmWriteMany,
                    &[&names.len().to_string(), &value_text],
                )
            };
            self.dialogs.confirm = Some(ConfirmRequest {
                title: tr(lang, Tr::ConfirmWriteTitle).to_string(),
                text,
                action: ConfirmAction::Write(request_list),
            });
            return;
        }
        self.dialogs.write.open = false;
        self.perform_write(request_list);
    }

    fn build_write_requests(&self, keys: &[ItemKey]) -> Result<PendingWrites, String> {
        let Some(row) = keys
            .first()
            .and_then(|key| self.rows.iter().find(|r| r.key == *key))
        else {
            return Err(tr(self.lang(), Tr::NoConnection).to_string());
        };
        let session = row.session;
        let Some(view) = self.session(session) else {
            return Ok(PendingWrites {
                session,
                protocol: Protocol::OpcDa,
                requests: Vec::new(),
                type_label: String::new(),
            });
        };
        let protocol = view.protocol();
        let choices = write_type_choices(protocol);
        let choice = choices
            .get(self.dialogs.write.type_index)
            .copied()
            .unwrap_or(choices[0]);

        let mut requests = Vec::new();
        for key in keys {
            let Some(row) = self.rows.iter().find(|r| r.key == *key) else {
                continue;
            };
            if !row.writable {
                return Err(fmt(self.lang(), Tr::NotWritable, &[&row.display_name]));
            }
            let value = parse_value(
                &self.dialogs.write.text,
                choice.kind,
                row.canonical_vt,
                row.data_type.as_deref(),
                protocol,
                self.lang(),
            )
            .map_err(|err| err)?;
            let type_hint = match protocol {
                Protocol::OpcDa => choice.vt.or(row.canonical_vt),
                Protocol::OpcUa => None,
            };
            requests.push(WriteRequest {
                key: *key,
                value,
                type_hint,
            });
        }
        Ok(PendingWrites {
            session,
            protocol,
            requests,
            type_label: choice.label.to_string(),
        })
    }

    pub(super) fn perform_write(&mut self, pending: PendingWrites) {
        let starting_new_batch = self.write_batch.pending == 0;
        let mut count = 0;
        for request in pending.requests {
            let key = request.key;
            if self
                .send_with(
                    pending.session,
                    pending.protocol,
                    CommandKind::Write { request },
                    PendingCtx::WriteKey(key),
                )
                .is_some()
            {
                self.write_batch.accepted(starting_new_batch && count == 0);
                count += 1;
            }
        }
        if count > 0 {
            self.dialogs.write.open = false;
        }
        let type_label = pending.type_label;
        if count > 0 {
            self.journal
                .info_l(Tr::WriteRequested, &[&count.to_string(), &type_label]);
        }
    }

    /// Execute a queued confirm-dialog action (Yes).
    pub fn confirm_yes(&mut self) {
        let Some(request) = self.dialogs.confirm.take() else {
            return;
        };
        match request.action {
            ConfirmAction::Write(pending) => self.perform_write(pending),
            ConfirmAction::Disconnect(session) => self.disconnect_session(session),
            ConfirmAction::RemoveSession(session) => self.remove_session(session),
        }
    }

    /// Cancel a confirm-dialog (No). Keeps the write dialog open for edits.
    pub fn confirm_no(&mut self) {
        self.dialogs.confirm = None;
    }

    /// Ask the user to confirm an action (opens the confirm dialog).
    pub fn request_confirm(&mut self, title: String, text: String, action: ConfirmAction) {
        self.dialogs.confirm = Some(ConfirmRequest {
            title,
            text,
            action,
        });
    }
}

impl App {
    pub fn selected_rows_for_context(&self, key: ItemKey) -> Vec<ItemKey> {
        if self.selected_rows.contains(&key) {
            self.selected_rows.iter().copied().collect()
        } else {
            vec![key]
        }
    }

    /// Key of the row at `index` in the table's visible list (V1).
    pub fn row_key(&self, index: usize) -> Option<ItemKey> {
        self.rows
            .iter()
            .filter(|row| self.row_shown(row))
            .nth(index)
            .map(|row| row.key)
    }

    pub fn select_only(&mut self, index: usize) {
        let Some(key) = self.row_key(index) else {
            return;
        };
        self.table_focus = Some(index);
        self.table_anchor = Some(index);
        self.tree_focus = None;
        self.tree_focused = false;
        self.selected_rows.clear();
        self.selected_rows.insert(key);
    }

    pub fn select_toggle(&mut self, index: usize) {
        let Some(key) = self.row_key(index) else {
            return;
        };
        self.table_focus = Some(index);
        self.table_anchor = Some(index);
        self.tree_focus = None;
        self.tree_focused = false;
        if !self.selected_rows.remove(&key) {
            self.selected_rows.insert(key);
        }
    }

    /// Select the visible rows from the anchor to `index` (both in the
    /// visible list), never rows of another tab.
    pub fn select_extend_to(&mut self, index: usize) {
        let visible = self.visible_row_positions();
        if visible.is_empty() {
            return;
        }
        let index = index.min(visible.len() - 1);
        let anchor = self.table_anchor.unwrap_or(index).min(visible.len() - 1);
        self.table_focus = Some(index);
        self.tree_focus = None;
        self.tree_focused = false;
        let (lo, hi) = if anchor <= index {
            (anchor, index)
        } else {
            (index, anchor)
        };
        self.selected_rows.clear();
        for position in visible.get(lo..=hi).unwrap_or_default() {
            if let Some(row) = self.rows.get(*position) {
                self.selected_rows.insert(row.key);
            }
        }
    }

    /// Select every row the table shows. The table keeps the keyboard (its
    /// focus stays on a row), so Delete / Enter / Ctrl+C act right away.
    pub fn select_all_rows(&mut self) {
        let keys: HashSet<ItemKey> = self
            .rows
            .iter()
            .filter(|row| self.row_shown(row))
            .map(|row| row.key)
            .collect();
        if keys.is_empty() {
            self.selected_rows.clear();
            return;
        }
        let count = keys.len();
        self.selected_rows = keys;
        self.table_focus = Some(self.table_focus.unwrap_or(0).min(count - 1));
        self.table_anchor = None;
        self.tree_focus = None;
        self.tree_focused = false;
    }

    pub fn clear_selection(&mut self) {
        self.selected_rows.clear();
    }

    pub fn copy_selected_rows(&self) -> Option<String> {
        if self.selected_rows.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        for row in self
            .rows
            .iter()
            .filter(|r| self.selected_rows.contains(&r.key))
        {
            lines.push(format!("{}\t{}", row.display_name, row.value));
        }
        Some(lines.join("\n"))
    }
}

fn parse_value(
    text: &str,
    kind: WriteKind,
    canonical_vt: Option<VarType>,
    ua_type: Option<&str>,
    protocol: Protocol,
    lang: Lang,
) -> Result<Value, String> {
    let kind = if kind == WriteKind::Auto {
        infer_kind(text, canonical_vt, ua_type, protocol)
    } else {
        kind
    };
    match kind {
        WriteKind::Auto => Ok(Value::Str(text.to_string())),
        WriteKind::Bool => {
            let value = text.trim().to_ascii_lowercase();
            match value.as_str() {
                "true" | "1" | "да" | "yes" | "是" | "on" => Ok(Value::Bool(true)),
                "false" | "0" | "нет" | "no" | "否" | "off" => Ok(Value::Bool(false)),
                _ => Err(fmt(lang, Tr::NotBool, &[&text])),
            }
        }
        WriteKind::I64 => text
            .trim()
            .parse::<i64>()
            .map(Value::I64)
            .map_err(|_| fmt(lang, Tr::NotInteger, &[&text])),
        WriteKind::U64 => text
            .trim()
            .parse::<u64>()
            .map(Value::U64)
            .map_err(|_| fmt(lang, Tr::NotUnsigned, &[&text])),
        WriteKind::F64 => text
            .trim()
            .replace(',', ".")
            .parse::<f64>()
            .map(Value::F64)
            .map_err(|_| fmt(lang, Tr::NotNumber, &[&text])),
        WriteKind::Str => Ok(Value::Str(text.to_string())),
        WriteKind::DateTime => parse_datetime(text, lang).map(Value::DateTime),
    }
}

pub(super) fn infer_kind(
    text: &str,
    canonical_vt: Option<VarType>,
    ua_type: Option<&str>,
    protocol: Protocol,
) -> WriteKind {
    if protocol == Protocol::OpcDa {
        if let Some(vt) = canonical_vt {
            match vt {
                VarType::BOOL => return WriteKind::Bool,
                VarType::I2 | VarType::I4 | VarType::I1 | VarType::I8 | VarType::INT => {
                    return WriteKind::I64;
                }
                VarType::UI1 | VarType::UI2 | VarType::UI4 | VarType::UI8 | VarType::UINT => {
                    return WriteKind::U64;
                }
                VarType::R4 | VarType::R8 | VarType::CY => return WriteKind::F64,
                VarType::DATE => return WriteKind::DateTime,
                VarType::BSTR | VarType::VARIANT | VarType::LPSTR | VarType::LPWSTR => {
                    return WriteKind::Str;
                }
                _ => {}
            }
        }
    } else if let Some(name) = ua_type {
        match name {
            "Boolean" => return WriteKind::Bool,
            "Byte" | "SByte" | "Int16" | "Int32" | "Int64" => return WriteKind::I64,
            "UInt16" | "UInt32" | "UInt64" => return WriteKind::U64,
            "Float" | "Double" => return WriteKind::F64,
            "DateTime" => return WriteKind::DateTime,
            "String" | "ByteString" | "LocalizedText" => return WriteKind::Str,
            _ => {}
        }
    }
    let trimmed = text.trim();
    if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "true" | "false" | "да" | "нет"
    ) {
        return WriteKind::Bool;
    }
    if trimmed.parse::<i64>().is_ok() {
        return WriteKind::I64;
    }
    if trimmed.parse::<u64>().is_ok() {
        return WriteKind::U64;
    }
    // A decimal comma is a number too (`3,14`); "inf"/"nan" stay strings.
    if trimmed.chars().any(|c| c.is_ascii_digit())
        && trimmed
            .replace(',', ".")
            .parse::<f64>()
            .is_ok_and(f64::is_finite)
    {
        return WriteKind::F64;
    }
    WriteKind::Str
}

fn parse_datetime(text: &str, lang: Lang) -> Result<DateTime<Utc>, String> {
    let text = text.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Ok(dt.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%d.%m.%Y %H:%M:%S",
        "%Y-%m-%d",
        "%d.%m.%Y",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(Local
                .from_local_datetime(&naive)
                .single()
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc.from_utc_datetime(&naive)));
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(text, format) {
            if let Some(naive) = date.and_hms_opt(0, 0, 0) {
                return Ok(Local
                    .from_local_datetime(&naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc.from_utc_datetime(&naive)));
            }
        }
    }
    Err(fmt(lang, Tr::NotDatetime, &[&text]))
}
