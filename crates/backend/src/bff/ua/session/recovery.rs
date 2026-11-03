//! O13/O27: reconcile only SDK facts for currently owned registrations.
use super::{HashMap, HashSet, ItemKey};

use std::sync::Arc;

// OPCLook identity budgets, not OPC UA protocol limits. The pool lives for
// the session so repeated Add/recovery batches cannot multiply URI storage.
const MAX_URI_BYTES: usize = 8 * 1024;
const MAX_URI_POOL_BYTES: usize = 1024 * 1024;
const MAX_URI_POOL_ENTRIES: usize = 4096;

#[derive(Default)]
pub(super) struct NamespacePool {
    strings: HashSet<Arc<str>>,
    bytes: usize,
}

impl NamespacePool {
    pub(super) fn snapshot<'a>(
        &mut self,
        namespaces: impl IntoIterator<Item = (&'a str, u16)>,
    ) -> HashMap<u16, Arc<str>> {
        let mut snapshot = HashMap::new();
        let mut seen = HashSet::new();
        for (uri, index) in namespaces {
            if !seen.insert(index) {
                snapshot.remove(&index);
                continue;
            }
            if uri.is_empty() || uri.len() > MAX_URI_BYTES {
                continue;
            }
            let value = if let Some(existing) = self.strings.get(uri) {
                existing.clone()
            } else {
                if self.strings.len() >= MAX_URI_POOL_ENTRIES
                    || self
                        .bytes
                        .checked_add(uri.len())
                        .is_none_or(|n| n > MAX_URI_POOL_BYTES)
                {
                    continue;
                }
                let value: Arc<str> = Arc::from(uri);
                self.bytes += uri.len();
                self.strings.insert(value.clone());
                value
            };
            snapshot.insert(index, value);
        }
        snapshot
    }
}

pub(super) fn namespace_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    matches!((expected, actual), (Some(a), Some(b)) if !a.is_empty() && a == b)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reconciled {
    pub monitored: HashMap<ItemKey, (u32, u32)>,
    pub subscriptions: Vec<u32>,
    pub requested: HashMap<u32, u64>,
    pub missing: Vec<ItemKey>,
    pub ambiguous: bool,
}

pub(super) fn reconcile(
    current: &HashSet<ItemKey>,
    old: &HashMap<ItemKey, (u32, u32)>,
    requested: &HashMap<u32, u64>,
    sdk_subscriptions: &[u32],
    snapshot: &[(u32, ItemKey, u32)],
) -> Reconciled {
    let mut result = Reconciled {
        monitored: HashMap::new(),
        subscriptions: Vec::new(),
        requested: HashMap::new(),
        missing: Vec::new(),
        ambiguous: false,
    };
    let mut duplicates = HashSet::new();
    let mut per_sub: HashMap<u32, Option<u64>> = HashMap::new();
    for &(sub, key, id) in snapshot {
        let Some((old_sub, _)) = old.get(&key) else {
            continue;
        };
        if !current.contains(&key) {
            continue;
        }
        if sub == 0 || id == 0 || duplicates.contains(&key) {
            result.ambiguous = true;
            duplicates.insert(key);
            result.monitored.remove(&key);
            continue;
        }
        if result.monitored.insert(key, (sub, id)).is_some() {
            duplicates.insert(key);
            result.monitored.remove(&key);
            result.ambiguous = true;
            continue;
        }
        let interval = requested.get(old_sub).copied();
        per_sub
            .entry(sub)
            .and_modify(|existing| {
                if *existing != interval {
                    *existing = None;
                }
            })
            .or_insert(interval);
    }
    for (&key, _) in old {
        if current.contains(&key) && !result.monitored.contains_key(&key) {
            result.missing.push(key);
        }
    }
    result.missing.sort_unstable();
    for &sub in sdk_subscriptions {
        if sub != 0 && !result.subscriptions.contains(&sub) {
            result.subscriptions.push(sub);
        }
    }
    result.subscriptions.sort_unstable();
    for &sub in &result.subscriptions {
        match per_sub.get(&sub).copied().flatten() {
            Some(interval) => {
                result.requested.insert(sub, interval);
            }
            None => result.ambiguous = true,
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(n: u32) -> ItemKey {
        ItemKey::new(n).unwrap()
    }
    #[test]
    fn namespace_pool_reuses_strings_and_fails_closed_at_budgets() {
        let mut pool = NamespacePool::default();
        let first = pool.snapshot([("urn:shared", 2), ("urn:other", 3)]);
        let again = pool.snapshot([("urn:shared", 9)]);
        assert!(Arc::ptr_eq(first.get(&2).unwrap(), again.get(&9).unwrap()));
        assert!(
            pool.snapshot([("urn:x", 5), ("urn:y", 5)])
                .get(&5)
                .is_none()
        );
        let oversized = "z".repeat(MAX_URI_BYTES + 1);
        assert!(pool.snapshot([(oversized.as_str(), 1)]).is_empty());

        let mut full = NamespacePool::default();
        let values: Vec<String> = (0..128)
            .map(|n| {
                let prefix = format!("{n:04}");
                format!("{}{}", prefix, "x".repeat(MAX_URI_BYTES - prefix.len()))
            })
            .collect();
        let exact = full.snapshot(
            values
                .iter()
                .enumerate()
                .map(|(i, uri)| (uri.as_str(), i as u16)),
        );
        assert_eq!(exact.len(), 128);
        assert_eq!(full.bytes, MAX_URI_POOL_BYTES);
        assert!(full.snapshot([("new", 500)]).is_empty());
        assert!(Arc::ptr_eq(
            exact.get(&0).unwrap(),
            full.snapshot([(values[0].as_str(), 0)]).get(&0).unwrap()
        ));
    }

    #[test]
    fn namespace_pool_entry_count_is_bounded() {
        let mut pool = NamespacePool::default();
        let values: Vec<String> = (0..=MAX_URI_POOL_ENTRIES)
            .map(|n| format!("urn:{n}"))
            .collect();
        let snapshot = pool.snapshot(
            values
                .iter()
                .enumerate()
                .map(|(i, uri)| (uri.as_str(), i as u16)),
        );
        assert_eq!(snapshot.len(), MAX_URI_POOL_ENTRIES);
        assert_eq!(pool.strings.len(), MAX_URI_POOL_ENTRIES);
        assert!(!snapshot.contains_key(&(MAX_URI_POOL_ENTRIES as u16)));
    }

    #[test]
    fn namespace_identity_requires_known_unchanged_uri() {
        assert!(namespace_matches(Some("urn:one"), Some("urn:one")));
        assert!(!namespace_matches(Some("urn:one"), Some("urn:other")));
        assert!(!namespace_matches(None, None));
        assert!(!namespace_matches(Some(""), Some("")));
        assert!(!namespace_matches(Some("urn:one"), None));
    }
    #[test]
    fn changed_ids_and_retired_keys_do_not_resurrect() {
        let current = [key(1)].into_iter().collect();
        let old = [(key(1), (10, 20)), (key(2), (10, 21))]
            .into_iter()
            .collect();
        let requested = [(10, 500)].into_iter().collect();
        let result = reconcile(
            &current,
            &old,
            &requested,
            &[30],
            &[(30, key(1), 40), (30, key(2), 41)],
        );
        assert_eq!(result.monitored.get(&key(1)), Some(&(30, 40)));
        assert!(!result.monitored.contains_key(&key(2)));
        assert_eq!(result.requested.get(&30), Some(&500));
        assert!(result.missing.is_empty());
        assert!(!result.ambiguous);
    }
    #[test]
    fn failed_and_duplicate_snapshot_cannot_claim_success() {
        let current = [key(1), key(2)].into_iter().collect();
        let old = [(key(1), (10, 20)), (key(2), (10, 21))]
            .into_iter()
            .collect();
        let requested = [(10, 500)].into_iter().collect();
        let result = reconcile(
            &current,
            &old,
            &requested,
            &[30, 31],
            &[(30, key(1), 40), (31, key(1), 41)],
        );
        assert!(result.monitored.is_empty());
        assert_eq!(result.missing, vec![key(1), key(2)]);
        assert!(result.ambiguous);
    }

    #[test]
    fn empty_or_mixed_requested_facts_are_not_invented() {
        let current = [key(1), key(2)].into_iter().collect();
        let old = [(key(1), (10, 20)), (key(2), (11, 21))]
            .into_iter()
            .collect();
        let requested = [(10, 500), (11, 1000)].into_iter().collect();
        let result = reconcile(
            &current,
            &old,
            &requested,
            &[30, 99],
            &[(30, key(1), 40), (30, key(2), 41)],
        );
        assert_eq!(result.subscriptions, vec![30, 99]);
        assert!(result.requested.is_empty());
        assert!(result.ambiguous);
        assert!(result.missing.is_empty());
    }
}
