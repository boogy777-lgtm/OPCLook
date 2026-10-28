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
        if node_id == BrowseNode::VIEWS_ROOT_ID {
            self.browse_views(session);
            return None;
        }
        // By NodeId only the first tree item carrying it is meant; callers that
        // know the exact item use `browse_item`.
        let item_id = self
            .sessions
            .iter()
            .find(|s| s.id == session)?
            .tree
            .iter()
            .find(|t| !node_id.is_empty() && t.node.node_id == node_id)
            .map(|t| t.id);
        self.browse_target(session, node_id, item_id)
    }

    /// Browse the children of one specific tree item (U7). The reply is routed
    /// back to this exact item even when the same NodeId occurs elsewhere.
    pub fn browse_item(&mut self, session: SessionId, item_id: u64) -> Option<RequestId> {
        let node_id = self
            .sessions
            .iter()
            .find(|s| s.id == session)?
            .tree
            .iter()
            .find(|t| t.id == item_id)?
            .node
            .node_id
            .clone();
        if node_id == BrowseNode::VIEWS_ROOT_ID {
            self.browse_views(session);
            return None;
        }
        self.browse_target(session, node_id, Some(item_id))
    }

    fn browse_target(
        &mut self,
        session: SessionId,
        node_id: String,
        item_id: Option<u64>,
    ) -> Option<RequestId> {
        let view = self.sessions.iter_mut().find(|s| s.id == session)?;
        if node_id.is_empty() {
            view.root_loading = true;
        } else if let Some(item) = item_id.and_then(|id| view.tree.iter_mut().find(|t| t.id == id))
        {
            item.loading = true;
        }
        let protocol = view.protocol();
        let filter = if protocol == Protocol::OpcUa {
            view.ua_browse_filter
        } else {
            crate::bff::model::NodeClassFilter::All
        };
        let view_id = if protocol == Protocol::OpcUa {
            view.ua_view_id.clone()
        } else {
            None
        };
        let ctx = match item_id {
            Some(id) if !node_id.is_empty() => PendingCtx::TreeItem(id),
            _ => PendingCtx::None,
        };
        let request_id = self.send_with(
            session,
            protocol,
            CommandKind::Browse {
                node_id,
                filter,
                view_id,
            },
            ctx,
        );
        self.engine
            .has_pending(session, request_id)
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
    ///
    /// U5: a changed filter re-reads the session's tree from the root (the
    /// already loaded nodes were fetched with the previous filter).
    pub fn set_ua_browse_vars_only(&mut self, session: SessionId, only: bool) {
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        let filter = if only {
            crate::bff::model::NodeClassFilter::Variables
        } else {
            crate::bff::model::NodeClassFilter::All
        };
        let changed = view.ua_browse_filter != filter;
        view.ua_browse_filter = filter;
        let live = view.state == SessionState::Connected && view.protocol() == Protocol::OpcUa;
        if changed && live {
            self.reread_tree(session);
        }
    }

    /// Drop the loaded tree of `session` and browse the root again.
    pub fn reread_tree(&mut self, session: SessionId) {
        self.cancel_add_children(session);
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.tree.clear();
            view.root_loaded = false;
        }
        self.selected_tree = self.selected_tree.filter(|(sid, _)| *sid != session);
        self.tree_focus = self.tree_focus.filter(|(sid, _)| *sid != session);
        self.browse(session, String::new());
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
        self.start_add_children_at(session, &node_id, Some(id));
    }

    /// Start (or restart) a recursive «Добавить дочерние» job for `branch_node`.
    ///
    /// A running job for the same session is cancelled first. The job walks the
    /// branch depth-first through the existing `Browse` event state machine:
    /// each browse response inserts the branch's children into the tree, adds
    /// every direct Item to the table, and enqueues every Branch child for a
    /// further browse. Cycles and duplicates are guarded by a visited set.
    pub fn start_add_children(&mut self, session: SessionId, branch_node: &str) {
        self.start_add_children_at(session, branch_node, None);
    }

    /// [`App::start_add_children`] for a known tree item (the one the user
    /// selected), so the first browse is routed to that exact item.
    fn start_add_children_at(
        &mut self,
        session: SessionId,
        branch_node: &str,
        item_id: Option<u64>,
    ) {
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
        let started = match item_id {
            Some(id) => self.browse_item(session, id),
            None => self.browse(session, branch_node.to_string()),
        };
        let Some(request_id) = started else {
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
        target_item: Option<u64>,
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
                let parent = match target_item {
                    Some(id) => view.tree.iter().find(|t| t.id == id),
                    None => view.tree.iter().find(|t| t.node.node_id == node_id),
                };
                if let Some(parent) = parent {
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
        if self.dialogs.add_signal.resolving.is_some() {
            // One resolution at a time; the dialog shows a spinner meanwhile.
            return;
        }
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
        // it first and add the first target when it arrives. The dialog stays
        // open until the answer (or its error) is in.
        if protocol == Protocol::OpcUa && !is_valid_ua_node_id(&item_id) {
            self.dialogs.add_signal.error = None;
            let path = if item_id.contains('/') {
                item_id.clone()
            } else {
                // A single segment has no namespace; browse from the root by name.
                format!("0:{item_id}")
            };
            let request_id = self.send_with(
                session,
                protocol,
                CommandKind::ResolveBrowsePath {
                    start_node: None,
                    path,
                },
                PendingCtx::ResolvePath {
                    requested: item_id,
                    name,
                },
            );
            if self.engine.has_pending(session, request_id) {
                self.dialogs.add_signal.resolving = Some((session, request_id));
            } else {
                // The command never left (dead worker): the journal has the
                // reason; say it in the dialog as well.
                self.dialogs.add_signal.error = Some(
                    self.sessions
                        .iter()
                        .find(|s| s.id == session)
                        .map(|s| s.info.clone())
                        .unwrap_or_default(),
                );
            }
            return;
        }
        self.add_items(session, vec![(item_id, name)]);
        self.close_add_signal_dialog();
    }

    /// Close «Добавить по имени» and reset its fields (also abandons a pending
    /// path resolution: its late answer is ignored).
    pub fn close_add_signal_dialog(&mut self) {
        let d = &mut self.dialogs.add_signal;
        d.open = false;
        d.resolving = None;
        d.item_id.clear();
        d.display_name.clear();
        d.error = None;
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

/// Index one past the last descendant of the item at `index`. The tree is kept
/// in depth-first order, so the descendants are exactly the contiguous run of
/// deeper items that follows the parent: O(subtree), no per-item parent walk.
pub(super) fn subtree_end(tree: &[TreeItem], index: usize) -> usize {
    let Some(parent) = tree.get(index) else {
        return index;
    };
    let depth = parent.depth;
    let mut end = index + 1;
    while tree.get(end).is_some_and(|item| item.depth > depth) {
        end += 1;
    }
    end
}

/// Replace the children of the item at `parent_index` with `children` in a
/// single splice (U6/A4). Any previous descendants of that item are dropped;
/// nothing outside its subtree is touched.
pub(super) fn set_children(
    view: &mut SessionView,
    parent_index: usize,
    children: Vec<BrowseNode>,
    loaded: bool,
) {
    let Some(parent) = view.tree.get(parent_index) else {
        return;
    };
    let parent_id = parent.id;
    let depth = parent.depth + 1;
    let end = subtree_end(&view.tree, parent_index);
    let mut id = view.next_tree_id;
    let new_items: Vec<TreeItem> = children
        .into_iter()
        .map(|node| {
            let item = TreeItem {
                id,
                parent: Some(parent_id),
                node,
                depth,
                expanded: false,
                loaded,
                loading: false,
            };
            id += 1;
            item
        })
        .collect();
    view.next_tree_id = id;
    view.tree.splice(parent_index + 1..end, new_items);
}

/// Insert (replacing previous ones) the browse result of the tree item with id
/// `parent_item`. Returns false when that item no longer exists (the tree was
/// refreshed meanwhile), so the caller can drop the stale reply.
pub(super) fn insert_children(
    view: &mut SessionView,
    parent_item: u64,
    children: Vec<BrowseNode>,
) -> bool {
    let Some(parent_index) = view.tree.iter().position(|t| t.id == parent_item) else {
        return false;
    };
    set_children(view, parent_index, children, false);
    true
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
