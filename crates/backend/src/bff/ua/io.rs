use super::common::timeout_drain;
use super::session::UaSession;
use super::values::{parse_node, ua_data_value_typed, value_to_ua_variant};
use crate::bff::model::{
    BackendMsg, BrowseNode, DataValue, ItemKey, NodeClassFilter, NodeKind, WriteRequest,
};
use crate::constants::{BROWSE_MAX_REFERENCES_PER_NODE, IO_TIMEOUT, MAX_BROWSE_NODES};
use opcua::types::{
    AttributeId, BrowseDescription, BrowseDirection, BrowsePath, BrowseResultMaskFlags,
    ContinuationPoint, DataValue as UaDataValue, NodeClass, NodeId, NumericRange, ObjectId,
    QualifiedName, ReadValueId, ReferenceTypeId, RelativePathElement, StatusCode,
    TimestampsToReturn, ViewDescription, WriteValue,
};
use std::collections::{HashMap, HashSet};

/// Node-class mask of a browse request. UA mask bits equal the `NodeClass`
/// values. The "variables" filter (`Variables`) keeps the Objects too: a Variable
/// is usually reached through folders, and a mask of Variable alone would hide
/// every folder and leave nothing to expand.
pub(super) fn browse_node_class_mask(filter: NodeClassFilter) -> u32 {
    let object = NodeClass::Object as i32 as u32;
    let variable = NodeClass::Variable as i32 as u32;
    match filter {
        NodeClassFilter::Objects => object,
        NodeClassFilter::Variables => object | variable,
        NodeClassFilter::All => 0,
    }
}

pub(super) async fn browse(
    session: &mut UaSession,
    node_id: &str,
    filter: NodeClassFilter,
    view_id: Option<String>,
) -> Result<Vec<BrowseNode>, BackendMsg> {
    let node = parse_node(node_id).unwrap_or_else(|| NodeId::from(ObjectId::RootFolder));
    let view = view_id
        .as_deref()
        .and_then(parse_node)
        .map(|view_id| ViewDescription {
            view_id,
            timestamp: Default::default(),
            view_version: 0,
        });
    let node_class_mask = browse_node_class_mask(filter);
    let description = BrowseDescription {
        node_id: node,
        browse_direction: BrowseDirection::Forward,
        reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
        include_subtypes: true,
        node_class_mask,
        result_mask: BrowseResultMaskFlags::all().bits(),
    };

    let browse_result = timeout_drain(
        session
            .session
            .browse(&[description], BROWSE_MAX_REFERENCES_PER_NODE, view),
        IO_TIMEOUT,
    )
    .await;
    let Ok(browse_result) = browse_result else {
        session.timed_out = true;
        return Err(BackendMsg::TimeoutBrowse);
    };
    let mut results = browse_result.map_err(|e| BackendMsg::UaServiceError {
        service: crate::bff::model::UaService::Browse,
        detail: e.to_string(),
    })?;

    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    loop {
        for result in &results {
            if result.status_code.is_bad() {
                continue;
            }
            for reference in result.references.iter().flatten() {
                if nodes.len() >= MAX_BROWSE_NODES {
                    break;
                }
                let id = reference.node_id.node_id.to_string();
                if !seen.insert(id.clone()) {
                    continue;
                }
                let display = {
                    let text = reference.display_name.text.to_string();
                    if text.trim().is_empty() {
                        let name = reference.browse_name.name.to_string();
                        if name.trim().is_empty() {
                            id.clone()
                        } else {
                            name
                        }
                    } else {
                        text
                    }
                };
                let is_item = reference.node_class == NodeClass::Variable;
                let has_children = matches!(
                    reference.node_class,
                    NodeClass::Object | NodeClass::Variable | NodeClass::View
                );
                nodes.push(BrowseNode {
                    node_id: id,
                    name: display,
                    kind: if is_item {
                        NodeKind::Item
                    } else {
                        NodeKind::Branch
                    },
                    has_children,
                });
            }
        }

        let points: Vec<ContinuationPoint> = results
            .iter()
            .filter_map(|r| {
                if r.continuation_point.value.is_some() {
                    Some(r.continuation_point.clone())
                } else {
                    None
                }
            })
            .collect();
        if points.is_empty() || nodes.len() >= MAX_BROWSE_NODES {
            if !points.is_empty() {
                let release =
                    timeout_drain(session.session.browse_next(true, &points), IO_TIMEOUT).await;
                if release.is_err() {
                    session.timed_out = true;
                }
            }
            break;
        }
        let next_result =
            timeout_drain(session.session.browse_next(false, &points), IO_TIMEOUT).await;
        let Ok(next_result) = next_result else {
            session.timed_out = true;
            return Err(BackendMsg::TimeoutBrowseNext);
        };
        results = next_result.map_err(|e| BackendMsg::UaServiceError {
            service: crate::bff::model::UaService::BrowseNext,
            detail: e.to_string(),
        })?;
    }

    Ok(nodes)
}

/// Lists the server's Views by browsing the standard Views folder reference
/// (`Root/Views`, ObjectId 84) and keeping only `NodeClass::View` references.
pub(super) async fn browse_views(session: &mut UaSession) -> Result<Vec<BrowseNode>, BackendMsg> {
    let views_node = NodeId::from(ObjectId::ViewsFolder);
    let description = BrowseDescription {
        node_id: views_node,
        browse_direction: BrowseDirection::Forward,
        reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
        include_subtypes: true,
        node_class_mask: 0,
        result_mask: BrowseResultMaskFlags::all().bits(),
    };
    let views_result = timeout_drain(
        session
            .session
            .browse(&[description], BROWSE_MAX_REFERENCES_PER_NODE, None),
        IO_TIMEOUT,
    )
    .await;
    let Ok(views_result) = views_result else {
        session.timed_out = true;
        return Err(BackendMsg::TimeoutViews);
    };
    let results = views_result.map_err(|e| BackendMsg::UaServiceError {
        service: crate::bff::model::UaService::BrowseViews,
        detail: e.to_string(),
    })?;

    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for result in &results {
        if result.status_code.is_bad() {
            continue;
        }
        for reference in result.references.iter().flatten() {
            if reference.node_class != NodeClass::View {
                continue;
            }
            let id = reference.node_id.node_id.to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            let display = {
                let text = reference.display_name.text.to_string();
                if text.trim().is_empty() {
                    let name = reference.browse_name.name.to_string();
                    if name.trim().is_empty() {
                        id.clone()
                    } else {
                        name
                    }
                } else {
                    text
                }
            };
            nodes.push(BrowseNode {
                node_id: id,
                name: display,
                kind: NodeKind::Branch,
                has_children: true,
            });
        }
    }
    // Reject an empty result: a server with no Views should surface a clear
    // message instead of silently returning nothing.
    if nodes.is_empty() {
        return Err(BackendMsg::NoViews);
    }
    Ok(nodes)
}

/// Resolves a relative BrowsePath (e.g. "0:Objects/2:MyVar") from `start_node`
/// (default root) into a list of target NodeId strings.
pub(super) async fn resolve_browse_path(
    session: &mut UaSession,
    start_node: Option<String>,
    path: &str,
) -> Result<Vec<String>, BackendMsg> {
    let path = path.trim();
    if path.is_empty() {
        return Err(BackendMsg::BrowsePathRequired);
    }
    let starting_node = start_node
        .as_deref()
        .and_then(parse_node)
        .unwrap_or_else(|| NodeId::from(ObjectId::RootFolder));
    let elements = parse_relative_path(path)?;
    let browse_path = BrowsePath {
        starting_node,
        relative_path: opcua::types::RelativePath {
            elements: Some(elements),
        },
    };
    let path_result = timeout_drain(
        session
            .session
            .translate_browse_paths_to_node_ids(&[browse_path]),
        IO_TIMEOUT,
    )
    .await;
    let Ok(path_result) = path_result else {
        session.timed_out = true;
        return Err(BackendMsg::TimeoutResolvePath);
    };
    let results = path_result.map_err(|e| BackendMsg::UaServiceError {
        service: crate::bff::model::UaService::TranslatePath,
        detail: e.to_string(),
    })?;

    let mut targets = Vec::new();
    for result in &results {
        if result.status_code.is_bad() {
            return Err(BackendMsg::ResolvePathFailed {
                status: format!(
                    "{} (0x{:08X})",
                    result.status_code.to_string(),
                    result.status_code.bits()
                ),
            });
        }
        for target in result.targets.iter().flatten() {
            let id = target.target_id.node_id.to_string();
            if !targets.contains(&id) {
                targets.push(id);
            }
        }
    }
    if targets.is_empty() {
        return Err(BackendMsg::PathNotFound);
    }
    Ok(targets)
}

/// Parses a relative browse path string of the form "0:Objects/2:MyVar" into
/// `RelativePathElement`s. Each element targets the `HierarchicalReferences`
/// (including subtypes) browse name given by `namespace:name`.
pub(super) fn parse_relative_path(path: &str) -> Result<Vec<RelativePathElement>, BackendMsg> {
    let mut elements = Vec::new();
    for part in path.split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (namespace_index, name) = match part.split_once(':') {
            Some((ns, name)) => {
                let ns: u16 = ns.trim().parse().map_err(|_| BackendMsg::PathBadNsPrefix {
                    part: part.to_string(),
                })?;
                (ns, name.trim().to_string())
            }
            None => (0, part.to_string()),
        };
        if name.is_empty() {
            return Err(BackendMsg::PathEmptyName {
                part: part.to_string(),
            });
        }
        elements.push(RelativePathElement {
            reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
            is_inverse: false,
            include_subtypes: true,
            target_name: QualifiedName::new(namespace_index, name),
        });
    }
    if elements.is_empty() {
        return Err(BackendMsg::PathEmpty);
    }
    Ok(elements)
}

pub(super) async fn read(session: &mut UaSession, keys: &[ItemKey]) -> Vec<(ItemKey, DataValue)> {
    let mut requests = Vec::new();
    let mut index_of: HashMap<ItemKey, usize> = HashMap::new();
    for key in keys {
        let Some(node_id) = session.items.get(key) else {
            continue;
        };
        index_of.insert(*key, requests.len());
        requests.push(ReadValueId {
            node_id: node_id.clone(),
            attribute_id: AttributeId::Value as u32,
            index_range: NumericRange::None,
            data_encoding: QualifiedName::null(),
        });
    }

    if requests.is_empty() {
        return keys
            .iter()
            .map(|key| (*key, DataValue::bad(BackendMsg::NodeNotSubscribed)))
            .collect();
    }

    let values = match session
        .read_values_chunked(&requests, TimestampsToReturn::Both, 0.0)
        .await
    {
        Ok(values) => values,
        Err(message) => {
            return keys
                .iter()
                .map(|key| {
                    if index_of.contains_key(key) {
                        (
                            *key,
                            DataValue::bad(BackendMsg::context("read", message.clone())),
                        )
                    } else {
                        (*key, DataValue::bad(BackendMsg::NodeNotSubscribed))
                    }
                })
                .collect();
        }
    };

    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let Some(index) = index_of.get(key) else {
            out.push((*key, DataValue::bad(BackendMsg::NodeNotSubscribed)));
            continue;
        };
        let Some(dv) = values.get(*index) else {
            out.push((*key, DataValue::bad(BackendMsg::ServerReturnedNoValue)));
            continue;
        };
        out.push((*key, ua_data_value_typed(dv, session.types.as_deref())));
    }
    out
}

pub(super) async fn write(
    session: &mut UaSession,
    request: WriteRequest,
) -> Result<crate::bff::model::WriteOutcome, BackendMsg> {
    let node_id = session
        .items
        .get(&request.key)
        .ok_or(BackendMsg::NodeNotSubscribed)?
        .clone();
    let data_type = session.node_types.get(&request.key).copied();
    let variant = value_to_ua_variant(&request.value, data_type)?;
    let value = UaDataValue {
        value: Some(variant),
        status: None,
        source_timestamp: None,
        source_picoseconds: None,
        server_timestamp: None,
        server_picoseconds: None,
    };
    let write = WriteValue {
        node_id,
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        value,
    };
    let write_result = timeout_drain(session.session.write(&[write]), IO_TIMEOUT).await;
    let Ok(write_result) = write_result else {
        session.timed_out = true;
        return Ok(crate::bff::model::WriteOutcome::unknown_because(
            BackendMsg::TimeoutWrite,
        ));
    };
    let result = match write_result {
        Ok(result) => result,
        Err(error) => {
            return Ok(crate::bff::model::WriteOutcome::unknown_with_status(
                crate::bff::model::NativeWriteStatus::UaService(error.status().bits()),
                error.to_string(),
            ));
        }
    };
    Ok(write_outcome(&result))
}

fn write_outcome(result: &[StatusCode]) -> crate::bff::model::WriteOutcome {
    match result.first() {
        Some(status) => crate::bff::model::WriteOutcome::from_ua(status.bits(), status.to_string()),
        None => crate::bff::model::WriteOutcome::unknown(""),
    }
}

#[cfg(test)]
mod write_outcome_tests {
    use super::*;
    use crate::bff::model::{NativeWriteStatus, WriteDisposition};

    #[test]
    fn adapter_keeps_good_async_uncertain_bad_and_missing() {
        for (raw, expected) in [
            (0, WriteDisposition::Accepted),
            (0x002e_0000, WriteDisposition::AcceptedAsynchronously),
            (0x4000_0000, WriteDisposition::Uncertain),
            (0x8000_0000, WriteDisposition::Rejected),
        ] {
            let outcome = write_outcome(&[StatusCode::from(raw)]);
            assert_eq!(outcome.raw, Some(NativeWriteStatus::Ua(raw)));
            assert_eq!(outcome.disposition, expected);
        }
        let missing = write_outcome(&[]);
        assert_eq!(missing.disposition, WriteDisposition::Unknown);
        assert_eq!(missing.raw, None);
    }
}

pub(super) async fn test(session: &mut UaSession) -> Result<BackendMsg, BackendMsg> {
    let node = NodeId::from(NodeId::new(0, 2256u32));
    let request = ReadValueId {
        node_id: node,
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        data_encoding: QualifiedName::null(),
    };
    let result = session
        .read_values_chunked(
            std::slice::from_ref(&request),
            TimestampsToReturn::Neither,
            0.0,
        )
        .await
        .map_err(|e| BackendMsg::context("test", e))?;
    match result.first() {
        Some(dv) if dv.status.map(|s| !s.is_bad()).unwrap_or(true) => Ok(BackendMsg::UaLinkOk),
        Some(dv) => Err(BackendMsg::UaBadStatus {
            status: dv.status.unwrap_or(StatusCode::Bad).to_string(),
        }),
        None => Err(BackendMsg::NoStatus),
    }
}
