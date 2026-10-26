use super::*;
use crate::bff::model::{BrowseNode, CommandKind, NodeKind, RequestId, SessionId};
use crate::constants::ADD_CHILDREN_CAP;
use crate::i18n::{Tr, tr};
use std::collections::HashSet;

impl App {
    /// Browse `node_id` and return the request id when the command was actually
    /// dispatched (`None` for the synthetic views root or a dead worker). The
    /// recursive add-children job uses this id to correlate its replies.
    pub fn browse(&mut self, session: SessionId, node_id: String) -> Option<RequestId> {
        if node_id == "\u{1}views" {
            self.browse_views(session);
            return None;
        }
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return None;
        };
        if node_id.is_empty() {
            view.root_loading = true;
        } else if let Some(item) = view.tree.iter_mut().find(|t| t.node.node_id == node_id) {
            item.loading = true;
        }
        let protocol = view.protocol();
        let filter = if protocol == Protocol::OpcUa && view.ua_browse_vars_only {
            Some(2)
        } else {
            None
        };
        let view_id = if protocol == Protocol::OpcUa {
            view.ua_view_id.clone()
        } else {
            None
        };
        let request_id = self.send(
            session,
            protocol,
            CommandKind::Browse {
                node_id,
                filter,
                view_id,
            },
        );
        self.pending_requests
            .contains(&(session, request_id))
            .then_some(request_id)
    }

    /// Requests the list of server Views (UA) and, on success, seeds the tree
    /// with a synthetic «Представления» root whose children are the server Views.
    pub fn browse_views(&mut self, session: SessionId) {
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        let protocol = view.protocol();
        if protocol == Protocol::OpcUa {
            view.root_loading = true;
        }
        self.send(session, protocol, CommandKind::BrowseViews);
    }

    /// Sets the active server View for UA browsing. `None` returns to the
    /// default (Objects/value) view.
    pub fn set_ua_view(&mut self, session: SessionId, view_id: Option<String>) {
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.ua_view_id = view_id;
        }
    }

    /// Selects a server View from a list and browses the root through it.
    /// `view_id = None` returns to the default view.
    pub fn view_browse(&mut self, session: SessionId, view_id: Option<String>) {
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        let protocol = view.protocol();
        if protocol != Protocol::OpcUa {
            return;
        }
        view.ua_view_id = view_id;
        self.browse(session, String::new());
    }

    /// Toggles the UA «Только переменные» browse filter for a session.
    pub fn set_ua_browse_vars_only(&mut self, session: SessionId, only: bool) {
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.ua_browse_vars_only = only;
        }
    }
}

impl App {
    pub fn tree_item(&self, session: SessionId, id: u64) -> Option<&TreeItem> {
        self.sessions
            .iter()
            .find(|s| s.id == session)?
            .tree
            .iter()
            .find(|t| t.id == id)
    }

    /// Returns the key of the table row already subscribed to the given tree
    /// item (same session and exact item id), if any.
    pub fn tree_row_key(&self, session: SessionId, item_id: &str) -> Option<ItemKey> {
        self.rows
            .iter()
            .find(|row| row.session == session && row.item_id == item_id)
            .map(|row| row.key)
    }

    pub fn add_selected_tree(&mut self) {
        let Some((session, id)) = self.selected_tree else {
            return;
        };
        let Some(item) = self.tree_item(session, id) else {
            return;
        };
        if item.node.kind != NodeKind::Item {
            let lang = self.lang();
            self.dialogs.message = Some(MessageDialog {
                title: tr(lang, Tr::AddSignalTitle).to_string(),
                text: tr(lang, Tr::AddSignalBadNode).to_string(),
                details: String::new(),
            });
            return;
        }
        let node_id = item.node.node_id.clone();
        let name = item.node.name.clone();
        self.add_items(session, vec![(node_id, name)]);
    }

    pub fn add_selected_children(&mut self) {
        let Some((session, id)) = self.selected_tree else {
            return;
        };
        let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        let Some(item) = view.tree.iter().find(|t| t.id == id) else {
            return;
        };
        if item.node.kind == NodeKind::Item {
            return;
        }
        let node_id = item.node.node_id.clone();
        self.start_add_children(session, &node_id);
    }

    /// Start (or restart) a recursive «Добавить дочерние» job for `branch_node`.
    ///
    /// A running job for the same session is cancelled first. The job walks the
    /// branch depth-first through the existing `Browse` event state machine:
    /// each browse response inserts the branch's children into the tree, adds
    /// every direct Item to the table, and enqueues every Branch child for a
    /// further browse. Cycles and duplicates are guarded by a visited set.
    pub fn start_add_children(&mut self, session: SessionId, branch_node: &str) {
        // A new job for the same session replaces the previous one.
        if let Some(job) = self.add_children.as_ref() {
            if job.session == session {
                self.add_children = None;
            }
        }
        let mut visited = HashSet::new();
        visited.insert(branch_node.to_string());
        // The start branch is not queued: it is already in flight. The queue
        // only holds descendants discovered while browsing.
        let queue = std::collections::VecDeque::new();
        // Issue the first browse and record its request id; the job only
        // consumes a Browse event whose id is in this map.
        let Some(request_id) = self.browse(session, branch_node.to_string()) else {
            return;
        };
        let mut in_flight = std::collections::HashMap::new();
        in_flight.insert(request_id, branch_node.to_string());
        self.add_children = Some(AddChildrenJob {
            session,
            queue,
            visited,
            in_flight,
            added: 0,
            cap: ADD_CHILDREN_CAP,
            capped: false,
        });
        self.journal
            .info_l(Tr::AddChildrenProgress, &[&"1".to_string()]);
    }

    /// Advance the recursive add-children job after a `Browse` response for a
    /// branch the job itself browsed. `request_id` is the correlation key: a
    /// reply for a manual/other browse is ignored even when the node id is the
    /// same.
    pub(super) fn advance_add_children(
        &mut self,
        session: SessionId,
        request_id: RequestId,
        node_id: &str,
    ) {
        let job_active = match self.add_children {
            Some(ref job) => job.session == session,
            None => false,
        };
        if !job_active {
            return;
        }
        let (items, keep) = {
            let Some(job) = self.add_children.as_mut() else {
                return;
            };
            // Only the exact browse the job issued may advance it.
            let Some(expected) = job.in_flight.remove(&request_id) else {
                return;
            };
            if expected != node_id {
                return;
            }
            // Collect direct Item children and Branch children of `node_id`.
            let mut items: Vec<(String, String)> = Vec::new();
            let mut branches: Vec<String> = Vec::new();
            if let Some(view) = self.sessions.iter().find(|s| s.id == session) {
                if let Some(parent) = view.tree.iter().find(|t| t.node.node_id == node_id) {
                    for child in view.tree.iter().filter(|t| t.parent == Some(parent.id)) {
                        match child.node.kind {
                            NodeKind::Item => {
                                items.push((child.node.node_id.clone(), child.node.name.clone()));
                            }
                            NodeKind::Branch if child.node.has_children => {
                                branches.push(child.node.node_id.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
            let cap_reached = job.visited.len() >= job.cap;
            for b in &branches {
                if job.visited.insert(b.clone()) {
                    job.queue.push_back(b.clone());
                }
            }
            (items, !cap_reached)
        };
        if !keep {
            if let Some(job) = self.add_children.as_mut() {
                if !job.capped {
                    job.capped = true;
                    self.journal
                        .warn_l(Tr::AddChildrenCapHit, &[&job.cap.to_string()]);
                }
            }
            // Drop queued work but keep processing in-flight browses.
            if let Some(job) = self.add_children.as_mut() {
                job.queue.clear();
            }
        }
        if !items.is_empty() {
            let before = self.rows.len();
            self.add_items(session, items);
            if let Some(job) = self.add_children.as_mut() {
                job.added += self.rows.len().saturating_sub(before);
            }
        }
        if keep {
            // Enqueue the next branch if any (drives the next browse), recording
            // the new request id for the same correlation rule.
            if let Some(next) = self.add_children.as_mut().and_then(|j| j.queue.pop_front()) {
                match self.browse(session, next.clone()) {
                    Some(rid) => {
                        if let Some(job) = self.add_children.as_mut() {
                            job.in_flight.insert(rid, next);
                        }
                    }
                    None => {
                        // Worker is gone: the job can never complete.
                        self.add_children = None;
                        return;
                    }
                }
            }
        }
        let done = {
            let job = self.add_children.as_ref();
            job.map(|j| j.queue.is_empty() && j.in_flight.is_empty())
                .unwrap_or(false)
        };
        if done {
            let added = self.add_children.as_ref().map(|j| j.added).unwrap_or(0);
            self.add_children = None;
            if added == 0 {
                self.journal.warn_l(Tr::FolChildrenNone, &[]);
            } else {
                self.journal.info_l(Tr::AddedSignals, &[&added.to_string()]);
            }
        } else {
            let n = self.add_children.as_ref().map(|j| j.added).unwrap_or(0);
            self.journal
                .info_l(Tr::AddChildrenProgress, &[&n.to_string()]);
        }
    }

    /// Cancel any running add-children job for `session` (on disconnect or a
    /// fresh job replacing it).
    pub fn cancel_add_children(&mut self, session: SessionId) {
        if let Some(job) = self.add_children.as_ref() {
            if job.session == session {
                self.add_children = None;
            }
        }
    }

    /// Add tree nodes (branches and/or items) received from a drag-and-drop
    /// onto the signal table. Only nodes of the active session are accepted.
    ///
    /// - Item nodes are added directly (deduplicated by `add_items`).
    /// - Branch nodes start a recursive add-children job («Добавить дочерние
    ///   сигналы» semantics) that walks every descendant, browsing on demand.
    pub fn add_dropped_tree(&mut self, session: SessionId, node_ids: Vec<u64>) {
        if self.active_session != Some(session) {
            self.journal.warn_l(Tr::DropCrossSession, &[]);
            return;
        }
        let mut items: Vec<(String, String)> = Vec::new();
        let mut branch_to_add: Option<String> = None;
        let mut had_items = false;
        let mut dropped_any = false;
        {
            let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
                return;
            };
            for id in &node_ids {
                let Some(item) = view.tree.iter().find(|t| &t.id == id) else {
                    continue;
                };
                dropped_any = true;
                match item.node.kind {
                    NodeKind::Item => {
                        items.push((item.node.node_id.clone(), item.node.name.clone()));
                        had_items = true;
                    }
                    NodeKind::Branch => {
                        if branch_to_add.is_none() {
                            branch_to_add = Some(item.node.node_id.clone());
                        }
                    }
                }
            }
        }
        if !items.is_empty() {
            self.add_items(session, items);
        }
        if let Some(branch_node) = branch_to_add {
            self.start_add_children(session, &branch_node);
        } else if dropped_any && !had_items {
            // Dropped only unknown/leaf nodes with nothing to add.
            self.journal.warn_l(Tr::DropEmptyBranch, &[]);
        }
    }

    pub fn add_by_name(&mut self) {
        let Some(session) = self.dialogs.add_signal.session else {
            self.dialogs.add_signal.error = Some(tr(self.lang(), Tr::NoServerSelected).to_string());
            return;
        };
        let item_id = self.dialogs.add_signal.item_id.trim().to_string();
        if item_id.is_empty() {
            self.dialogs.add_signal.error = Some(tr(self.lang(), Tr::NameRequired).to_string());
            return;
        }
        let name = self.dialogs.add_signal.display_name.trim().to_string();
        let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        let protocol = view.protocol();
        // For UA, a bare name that is not a valid NodeId is treated as a
        // relative BrowsePath from the root (e.g. "0:Objects/2:MyVar"); resolve
        // it first and add the first target when it arrives.
        if protocol == Protocol::OpcUa && !is_valid_ua_node_id(&item_id) {
            self.pending_path_resolve = Some((session, item_id.clone(), name));
            self.dialogs.add_signal.error = None;
            let path = if item_id.contains('/') {
                item_id.clone()
            } else {
                // A single segment has no namespace; browse from the root by name.
                format!("0:{item_id}")
            };
            self.send(
                session,
                protocol,
                CommandKind::ResolveBrowsePath {
                    start_node: None,
                    path,
                },
            );
            self.dialogs.add_signal.open = false;
            self.dialogs.add_signal.item_id.clear();
            self.dialogs.add_signal.display_name.clear();
            self.dialogs.add_signal.error = None;
            return;
        }
        self.add_items(session, vec![(item_id, name)]);
        self.dialogs.add_signal.open = false;
        self.dialogs.add_signal.item_id.clear();
        self.dialogs.add_signal.display_name.clear();
        self.dialogs.add_signal.error = None;
    }
}

impl App {
    pub fn set_tree_focus(&mut self, session: SessionId, id: u64) {
        self.tree_focused = true;
        self.tree_focus = Some((session, id));
        self.selected_tree = Some((session, id));
        self.active_session = Some(session);
        self.table_focus = None;
        self.table_anchor = None;
    }
}

pub(super) fn replace_roots(view: &mut SessionView, children: Vec<BrowseNode>) {
    view.tree.clear();
    let mut id = view.next_tree_id;
    for node in children {
        view.tree.push(TreeItem {
            id,
            parent: None,
            node,
            depth: 0,
            expanded: false,
            loaded: false,
            loading: false,
        });
        id += 1;
    }
    view.next_tree_id = id;
}

pub(super) fn insert_children(
    view: &mut SessionView,
    parent_node: &str,
    children: Vec<BrowseNode>,
) {
    let Some(parent_index) = view.tree.iter().position(|t| t.node.node_id == parent_node) else {
        return;
    };
    let parent_id = view.tree[parent_index].id;
    let parent_depth = view.tree[parent_index].depth;

    let old_ids: Vec<u64> = view
        .tree
        .iter()
        .filter(|item| is_descendant(view, item.id, parent_id))
        .map(|item| item.id)
        .collect();
    view.tree.retain(|item| !old_ids.contains(&item.id));

    let parent_index = view
        .tree
        .iter()
        .position(|t| t.id == parent_id)
        .unwrap_or(parent_index);
    let mut insert_at = parent_index + 1;
    while insert_at < view.tree.len() && view.tree[insert_at].depth > parent_depth {
        insert_at += 1;
    }

    let mut new_items = Vec::new();
    let mut id = view.next_tree_id;
    for node in children {
        new_items.push(TreeItem {
            id,
            parent: Some(parent_id),
            node,
            depth: parent_depth + 1,
            expanded: false,
            loaded: false,
            loading: false,
        });
        id += 1;
    }
    view.next_tree_id = id;
    view.tree.splice(insert_at..insert_at, new_items);
}

fn is_descendant(view: &SessionView, item_id: u64, ancestor: u64) -> bool {
    let mut current = item_id;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 100_000 {
            return false;
        }
        let Some(item) = view.tree.iter().find(|t| t.id == current) else {
            return false;
        };
        match item.parent {
            Some(parent) if parent == ancestor => return true,
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Best-effort check whether `text` is a legal OPC UA NodeId string
/// (`i=…`, `ns=…;i=…`, `s=…`, `ns=…;s=…`, `g=…`, `b=…`). Used to decide
/// whether "Добавить по имени" should treat the input as a direct NodeId or
/// as a relative BrowsePath to resolve.
fn is_valid_ua_node_id(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    let id_part = if let Some((_, rest)) = text.split_once(';') {
        rest.trim()
    } else {
        text
    };
    let Some((tag, value)) = id_part.split_once('=') else {
        return false;
    };
    let value = value.trim();
    match tag.trim() {
        "i" | "g" => value.parse::<u32>().is_ok() || !value.is_empty(),
        "s" | "b" => !value.is_empty(),
        _ => false,
    }
}
