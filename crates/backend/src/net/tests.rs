use std::collections::HashSet;
use std::net::IpAddr;
use std::num::NonZeroU32;

use super::*;

fn addr(ip: &str, prefix: u8) -> AdapterAddress {
    AdapterAddress {
        ip: ip.parse().unwrap(),
        prefix,
        scope_id: None,
    }
}

fn adapter(
    id: AdapterId,
    name: &str,
    is_physical: bool,
    is_loopback: bool,
    up: bool,
    addresses: Vec<AdapterAddress>,
) -> AdapterInfo {
    AdapterInfo {
        id,
        name: name.to_string(),
        description: String::new(),
        state: if up {
            AdapterState::Up
        } else {
            AdapterState::Down
        },
        if_type: if is_loopback { 24 } else { 6 },
        addresses,
        gateways: Vec::new(),
        if_index: std::num::NonZeroU32::new(1),
        ipv6_if_index: std::num::NonZeroU32::new(2),
        luid: 1,
        metric: 10,
        is_physical,
        is_loopback,
    }
}

fn id(text: &str) -> AdapterId {
    AdapterId::parse(text).unwrap()
}

#[test]
fn legacy_interface_indexes_and_zero_scope_round_trip() {
    let raw = r#"{"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","if_index":0,"ipv6_if_index":12,"addresses":[{"ip":"fe80::1","prefix":64,"scope_id":0}]}"#;
    let cached: CachedAdapter = serde_json::from_str(raw).unwrap();
    assert_eq!(cached.if_index, None);
    assert_eq!(cached.ipv6_if_index, NonZeroU32::new(12));
    assert_eq!(cached.addresses[0].scope_id, None);
    let saved = serde_json::to_value(&cached).unwrap();
    assert_eq!(saved["if_index"], 0);
    assert_eq!(saved["ipv6_if_index"], 12);
    assert!(saved["addresses"][0]["scope_id"].is_null());
    let binding = cached.to_info().network_binding_for(cached.addresses[0]);
    let remote: std::net::SocketAddr = "[fe80::2]:4840".parse().unwrap();
    assert_eq!(
        binding.prepare_remote(remote).to_string(),
        "[fe80::2%12]:4840"
    );
    assert_eq!(
        binding.local_socket_addr(remote.ip()).unwrap().to_string(),
        "[fe80::1%12]:0"
    );
}

#[test]
fn adapter_id_parses_and_formats_canonically() {
    let text = "AABBCCDD-1122-3344-5566-77889900AABB";
    let parsed = id(text);
    assert_eq!(parsed.to_string(), "aabbccdd-1122-3344-5566-77889900aabb");
    let braced = AdapterId::parse("{aabbccdd-1122-3344-5566-77889900aabb}").unwrap();
    assert_eq!(parsed, braced);
    assert!(!parsed.is_unspecified());
    let json = serde_json::to_string(&parsed).unwrap();
    let back: AdapterId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, parsed);
}

#[test]
fn adapter_id_rejects_malformed_values_without_panicking() {
    for raw in [
        "",
        "not-a-guid",
        "aabbccdd-1122-3344-5566",
        "zzbcccdd-1122-3344-5566-77889900aabb",
        "ébbccdd-1122-3344-5566-77889900aabb",
        "{aabbccdd-1122-3344-5566-77889900aabb",
        "aabbccdd-1122-3344-5566-77889900aabb-extra",
    ] {
        assert!(AdapterId::parse(raw).is_err(), "accepted {raw:?}");
    }
    assert!(AdapterId::default().is_unspecified());
}

#[test]
fn subnet_sizes_and_hosts_for_common_masks() {
    let s23 = SubnetSpec::new("192.168.2.10".parse().unwrap(), 23).unwrap();
    assert_eq!(s23.network().to_string(), "192.168.2.0");
    assert_eq!(s23.capacity(), 512);
    assert_eq!(s23.usable_count(), 510);
    assert_eq!(s23.hosts().count(), 510);
    assert!(s23.contains("192.168.3.200".parse().unwrap()));
    assert!(!s23.contains("192.168.4.1".parse().unwrap()));
    assert!(!s23.needs_confirmation());

    let s24 = SubnetSpec::new("10.0.0.7".parse().unwrap(), 24).unwrap();
    assert_eq!(s24.capacity(), 256);
    assert_eq!(s24.usable_count(), 254);
    assert_eq!(s24.hosts().next().unwrap().to_string(), "10.0.0.1");
    assert_eq!(s24.hosts().last().unwrap().to_string(), "10.0.0.254");
}

#[test]
fn subnet_31_and_32_keep_all_addresses() {
    let s31 = SubnetSpec::new("10.0.0.4".parse().unwrap(), 31).unwrap();
    assert_eq!(s31.capacity(), 2);
    assert_eq!(s31.usable_count(), 2);
    assert_eq!(s31.hosts().collect::<Vec<_>>().len(), 2);
    assert!(s31.contains("10.0.0.5".parse().unwrap()));

    let s32 = SubnetSpec::new("10.0.0.4".parse().unwrap(), 32).unwrap();
    assert_eq!(s32.capacity(), 1);
    assert_eq!(s32.usable_count(), 1);
    assert_eq!(s32.hosts().collect::<Vec<_>>().len(), 1);
}

#[test]
fn subnet_ipv6_and_huge_ranges_do_not_enumerate_eagerly() {
    let s64 = SubnetSpec::new("fe80::1".parse().unwrap(), 64).unwrap();
    assert_eq!(s64.capacity(), u64::MAX);
    assert!(s64.needs_confirmation());
    assert!(s64.contains("fe80::abcd".parse().unwrap()));
    assert_eq!(s64.hosts().take(3).count(), 3);

    let s128 = SubnetSpec::host("::1".parse().unwrap());
    assert_eq!(s128.capacity(), 1);
    assert_eq!(s128.usable_count(), 1);
    assert_eq!(
        s128.hosts().collect::<Vec<_>>(),
        vec!["::1".parse::<IpAddr>().unwrap()]
    );

    let tail = SubnetSpec::new(
        "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fffe".parse().unwrap(),
        127,
    )
    .unwrap();
    assert_eq!(tail.hosts().collect::<Vec<_>>().len(), 2);
}

#[test]
fn subnet_zero_prefix_and_ipv6_max_iterator_boundaries() {
    let v4 = SubnetSpec::new("192.0.2.1".parse().unwrap(), 0).unwrap();
    assert_eq!(v4.network().to_string(), "0.0.0.0");
    assert!(v4.contains("255.255.255.255".parse().unwrap()));
    let v6 = SubnetSpec::new("2001:db8::1".parse().unwrap(), 0).unwrap();
    assert_eq!(v6.network().to_string(), "::");
    assert!(v6.contains("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()));
    let max = SubnetSpec::host("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap());
    let mut hosts = max.hosts();
    assert_eq!(hosts.next(), Some(max.network()));
    assert_eq!(hosts.next(), None);
    assert_eq!(hosts.next(), None);
}

#[test]
fn invalid_prefix_is_an_error_not_a_panic() {
    assert!(SubnetSpec::new("10.0.0.1".parse().unwrap(), 33).is_err());
    assert!(SubnetSpec::new("::1".parse().unwrap(), 129).is_err());
}

#[test]
fn none_selection_is_an_explicit_error_without_fallback() {
    let physical = id("11111111-1111-1111-1111-111111111111");
    let inv = NetworkInventory::new(
        vec![adapter(
            physical,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )],
        7,
    );
    assert_eq!(
        inv.resolve(&AdapterSelection::None, None),
        Err(NetError::NoAdapterSelected)
    );
}

#[test]
fn manual_selection_is_used_when_available() {
    let selected = id("11111111-1111-1111-1111-111111111111");
    let inv = NetworkInventory::new(
        vec![adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )],
        7,
    );
    let lease = inv
        .resolve(&AdapterSelection::Adapter(selected), None)
        .unwrap();
    assert_eq!(lease.generation, 7);
    assert_eq!(lease.adapter.id, selected);
    assert_eq!(lease.local_ip().to_string(), "192.168.1.20");
    assert_eq!(lease.requested, AdapterSelection::Adapter(selected));
}

#[test]
fn missing_or_down_adapter_is_an_error_not_a_fallback() {
    let missing = id("22222222-2222-2222-2222-222222222222");
    let down = id("55555555-5555-5555-5555-555555555555");
    let usable = id("66666666-6666-6666-6666-666666666666");
    let inv = NetworkInventory::new(
        vec![
            adapter(
                down,
                "Ethernet 2",
                true,
                false,
                false,
                vec![addr("192.168.2.10", 24)],
            ),
            adapter(
                usable,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            ),
        ],
        4,
    );
    assert_eq!(
        inv.resolve(&AdapterSelection::Adapter(missing), None),
        Err(NetError::AdapterNotFound {
            adapter: missing.to_string()
        })
    );
    assert_eq!(
        inv.resolve(&AdapterSelection::Adapter(down), None),
        Err(NetError::AdapterUnavailable {
            adapter: down.to_string()
        })
    );
}

#[test]
fn preferred_ip_is_used_when_present() {
    let selected = id("11111111-1111-1111-1111-111111111111");
    let inv = NetworkInventory::new(
        vec![adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24), addr("192.168.1.21", 24)],
        )],
        1,
    );
    let preferred: IpAddr = "192.168.1.21".parse().unwrap();
    let lease = inv
        .resolve(&AdapterSelection::Adapter(selected), Some(preferred))
        .unwrap();
    assert_eq!(lease.local_ip(), preferred);
    assert_eq!(lease.subnet.prefix(), 24);
}

#[test]
fn address_preference_avoids_apipa_and_uses_ipv4_first() {
    let selected = id("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    let info = adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![
            addr("fe80::1", 64),
            addr("169.254.10.10", 16),
            addr("192.168.1.20", 24),
        ],
    );
    let selected_address = info.select_address(None).unwrap();
    assert_eq!(selected_address.ip.to_string(), "192.168.1.20");
}

#[test]
fn network_state_advances_generation_and_clears_lease_without_selection() {
    let selected = id("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    let mut state = NetworkState::default();
    state.adapters = vec![adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![addr("192.168.1.20", 24)],
    )];
    state.resolve(AdapterSelection::None, None);
    let first = state.generation;
    assert!(state.lease.is_none());
    assert!(state.error.is_none());
    state.resolve(AdapterSelection::Adapter(selected), None);
    assert!(state.generation > first);
    assert!(state.lease.is_some());
    assert!(state.error.is_none());
}

#[test]
fn successful_resolve_clears_a_previous_network_error() {
    let selected = id("01010101-0101-0101-0101-010101010101");
    let missing = id("02020202-0202-0202-0202-020202020202");
    let mut state = NetworkState::default();
    state.set_adapters(vec![adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![addr("192.168.1.20", 24)],
    )]);
    state.resolve(AdapterSelection::Adapter(missing), None);
    assert!(
        state.error.is_some(),
        "missing adapter must report an error"
    );
    state.resolve(AdapterSelection::Adapter(selected), None);
    assert!(state.lease.is_some());
    assert!(state.error.is_none(), "successful resolve clears the error");
}

#[test]
fn build_adapter_cache_replaces_and_filters_inventory() {
    let up = id("10000000-0000-0000-0000-000000000001");
    let down = id("10000000-0000-0000-0000-000000000002");
    let no_address = id("10000000-0000-0000-0000-000000000003");
    let loopback = id("10000000-0000-0000-0000-000000000004");
    let duplicate = id("10000000-0000-0000-0000-000000000005");
    let inventory = vec![
        adapter(
            up,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        ),
        adapter(
            down,
            "Ethernet 2",
            true,
            false,
            false,
            vec![addr("10.0.0.2", 24)],
        ),
        adapter(no_address, "VPN", false, false, true, Vec::new()),
        adapter(
            loopback,
            "Loopback",
            false,
            true,
            true,
            vec![addr("127.0.0.1", 8)],
        ),
        adapter(
            duplicate,
            "Duplicate A",
            true,
            false,
            true,
            vec![addr("172.16.0.2", 24)],
        ),
        adapter(
            duplicate,
            "Duplicate B",
            true,
            false,
            true,
            vec![addr("172.16.0.3", 24)],
        ),
    ];
    let build = build_adapter_cache(&inventory);
    assert_eq!(
        build.cached.len(),
        1,
        "only the usable unique adapter remains"
    );
    assert_eq!(build.cached[0].id, up);
    let reasons: Vec<CacheSkipReason> = build.skipped.iter().map(|skip| skip.reason).collect();
    assert!(reasons.contains(&CacheSkipReason::NotUp));
    assert!(reasons.contains(&CacheSkipReason::NoAddress));
    assert!(reasons.contains(&CacheSkipReason::Loopback));
    assert_eq!(
        reasons
            .iter()
            .filter(|reason| **reason == CacheSkipReason::DuplicateId)
            .count(),
        2
    );

    // A second refresh fully replaces the cache; it is never a union.
    let replacement = build_adapter_cache(&[adapter(
        no_address,
        "Ethernet 3",
        true,
        false,
        true,
        vec![addr("192.168.5.10", 24)],
    )]);
    assert_eq!(replacement.cached.len(), 1);
    assert_eq!(replacement.cached[0].id, no_address);
    assert!(replacement.cached.iter().all(|item| item.id != up));
}

#[test]
fn load_cached_adapters_rejects_duplicates_unspecified_down_and_no_address() {
    let up = id("20000000-0000-0000-0000-000000000001");
    let down = id("20000000-0000-0000-0000-000000000002");
    let no_address = id("20000000-0000-0000-0000-000000000003");
    let duplicate = id("20000000-0000-0000-0000-000000000004");
    let cached = vec![
        CachedAdapter::from_info(&adapter(
            up,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )),
        CachedAdapter::from_info(&adapter(
            duplicate,
            "Duplicate A",
            true,
            false,
            true,
            vec![addr("10.0.0.2", 24)],
        )),
        CachedAdapter::from_info(&adapter(
            duplicate,
            "Duplicate B",
            true,
            false,
            true,
            vec![addr("10.0.0.3", 24)],
        )),
        CachedAdapter::from_info(&adapter(
            down,
            "Ethernet 2",
            true,
            false,
            false,
            vec![addr("10.0.0.4", 24)],
        )),
        CachedAdapter::from_info(&adapter(no_address, "VPN", false, false, true, Vec::new())),
        CachedAdapter::from_info(&adapter(
            AdapterId::UNSPECIFIED,
            "Loopback",
            false,
            true,
            true,
            vec![addr("127.0.0.1", 8)],
        )),
    ];
    let (adapters, report) = load_cached_adapters(&cached);
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].id, up);
    assert_eq!(report.accepted, 1);
    assert_eq!(report.rejected.len(), 5);
}

#[test]
fn adapter_cache_loads_without_windows_and_keeps_ipv6_scope() {
    let selected = id("30000000-0000-0000-0000-000000000001");
    let mut link_local = addr("fe80::10", 64);
    link_local.scope_id = std::num::NonZeroU32::new(12);
    let cached = CachedAdapter::from_info(&adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![link_local],
    ));
    let mut state = NetworkState::default();
    let report = state.load_cache(&[cached]);
    assert_eq!(report.accepted, 1);
    assert!(state.loaded);
    assert_eq!(state.adapters.len(), 1);
    let resolution = reconcile_selection(
        &state.adapters,
        &AdapterSelection::Adapter(selected),
        Some("fe80::10".parse().unwrap()),
    );
    assert_eq!(resolution.state, SelectionState::Selected);
    state.resolve(resolution.selection, resolution.local_ip);
    let lease = state.lease.expect("lease");
    assert_eq!(lease.local_ip().to_string(), "fe80::10");
    assert_eq!(
        lease.network_binding().scope_id,
        std::num::NonZeroU32::new(12)
    );
}

#[test]
fn stale_guid_has_zero_selected_rows_and_no_fallback() {
    let available = id("40000000-0000-0000-0000-000000000001");
    let stale = id("40000000-0000-0000-0000-000000000002");
    let adapters = vec![adapter(
        available,
        "Ethernet",
        true,
        false,
        true,
        vec![addr("192.168.1.20", 24)],
    )];
    let selection = AdapterSelection::Adapter(stale);
    assert_eq!(selected_adapter_count(&adapters, &selection), 0);
    assert!(selected_adapter(&adapters, &selection).is_none());
    let resolution = reconcile_selection(&adapters, &selection, Some("10.0.0.9".parse().unwrap()));
    assert_eq!(resolution.state, SelectionState::Stale);
    assert_eq!(resolution.selection, AdapterSelection::None);
    assert!(resolution.local_ip.is_none());
}

#[test]
fn exactly_one_row_is_selected_even_with_duplicate_guids() {
    let id_a = id("50000000-0000-0000-0000-000000000001");
    let id_b = id("50000000-0000-0000-0000-000000000002");
    let adapters = vec![
        adapter(
            id_a,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        ),
        adapter(
            id_a,
            "Ethernet duplicate",
            true,
            false,
            true,
            vec![addr("192.168.1.21", 24)],
        ),
        adapter(id_b, "Wi-Fi", true, false, true, vec![addr("10.0.0.5", 24)]),
    ];
    assert_eq!(selectable_adapters(&adapters).len(), 2);
    assert_eq!(
        selected_adapter_count(&adapters, &AdapterSelection::Adapter(id_a)),
        1
    );
    assert_eq!(
        selected_adapter_count(&adapters, &AdapterSelection::Adapter(id_b)),
        1
    );
    assert_eq!(
        selected_adapter_count(&adapters, &AdapterSelection::None),
        0
    );
    assert_eq!(
        selected_adapter_count(
            &adapters,
            &AdapterSelection::Adapter(AdapterId::UNSPECIFIED)
        ),
        0
    );
}

#[test]
fn ip_must_belong_to_the_selected_adapter() {
    let ethernet = id("60000000-0000-0000-0000-000000000001");
    let wifi = id("60000000-0000-0000-0000-000000000002");
    let adapters = vec![
        adapter(
            ethernet,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        ),
        adapter(wifi, "Wi-Fi", true, false, true, vec![addr("10.0.0.5", 24)]),
    ];
    // A foreign IP is ignored and the selected adapter's first address wins.
    let a = reconcile_selection(
        &adapters,
        &AdapterSelection::Adapter(ethernet),
        Some("10.0.0.5".parse().unwrap()),
    );
    assert_eq!(a.local_ip, Some("192.168.1.20".parse().unwrap()));
    let b = reconcile_selection(
        &adapters,
        &AdapterSelection::Adapter(wifi),
        Some("192.168.1.20".parse().unwrap()),
    );
    assert_eq!(b.local_ip, Some("10.0.0.5".parse().unwrap()));
    // A saved IP that belongs to the selected adapter is preserved.
    let keep = reconcile_selection(
        &adapters,
        &AdapterSelection::Adapter(ethernet),
        Some("192.168.1.20".parse().unwrap()),
    );
    assert_eq!(keep.local_ip, Some("192.168.1.20".parse().unwrap()));
}

#[test]
fn multiple_addresses_select_ip_and_matching_subnet() {
    let selected = id("70000000-0000-0000-0000-000000000001");
    let adapters = vec![adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![addr("192.168.1.20", 24), addr("192.168.2.30", 16)],
    )];
    let resolution = reconcile_selection(
        &adapters,
        &AdapterSelection::Adapter(selected),
        Some("192.168.2.30".parse().unwrap()),
    );
    assert_eq!(resolution.local_ip, Some("192.168.2.30".parse().unwrap()));
    let lease = NetworkInventory::new(adapters, 1)
        .resolve(&resolution.selection, resolution.local_ip)
        .unwrap();
    assert_eq!(lease.local_ip().to_string(), "192.168.2.30");
    assert_eq!(lease.subnet.prefix(), 16);
}

#[test]
fn loopback_is_not_a_selectable_adapter() {
    let loopback = id("80000000-0000-0000-0000-000000000001");
    let adapters = vec![adapter(
        loopback,
        "Loopback",
        false,
        true,
        true,
        vec![addr("127.0.0.1", 8)],
    )];
    assert!(selectable_adapters(&adapters).is_empty());
    let resolution = reconcile_selection(&adapters, &AdapterSelection::Adapter(loopback), None);
    assert_eq!(resolution.state, SelectionState::Stale);
}

#[test]
fn empty_cache_yields_no_selection() {
    let adapters: Vec<AdapterInfo> = Vec::new();
    assert!(selectable_adapters(&adapters).is_empty());
    let resolution = reconcile_selection(&adapters, &AdapterSelection::None, None);
    assert_eq!(resolution.state, SelectionState::NotSelected);
    assert!(resolution.local_ip.is_none());
}

#[test]
fn windows_inventory_is_non_panicking_without_a_selection() {
    let inventory = match inventory_adapters() {
        Ok(adapters) => {
            let mut ids = HashSet::new();
            for adapter in &adapters {
                assert!(
                    !adapter.id.is_unspecified(),
                    "adapter {:?} has no connection GUID",
                    adapter.name
                );
                assert!(
                    ids.insert(adapter.id),
                    "duplicate adapter GUID {} for {:?}",
                    adapter.id,
                    adapter.name
                );
            }
            NetworkInventory::new(adapters, 1)
        }
        Err(_) => NetworkInventory::default(),
    };
    assert_eq!(
        inventory.resolve(&AdapterSelection::None, None),
        Err(NetError::NoAdapterSelected)
    );
}

#[test]
fn lease_binding_carries_both_interface_indexes() {
    let selected = id("cccccccc-cccc-cccc-cccc-cccccccccccc");
    let mut info = adapter(
        selected,
        "Ethernet",
        true,
        false,
        true,
        vec![addr("192.168.1.20", 24)],
    );
    info.if_index = std::num::NonZeroU32::new(11);
    info.ipv6_if_index = std::num::NonZeroU32::new(17);
    let inv = NetworkInventory::new(vec![info], 1);
    let lease = inv
        .resolve(&AdapterSelection::Adapter(selected), None)
        .unwrap();
    let binding = lease.network_binding();
    assert_eq!(binding.local_ip.to_string(), "192.168.1.20");
    assert_eq!(binding.ipv4_if_index, std::num::NonZeroU32::new(11));
    assert_eq!(binding.ipv6_if_index, std::num::NonZeroU32::new(17));
    assert_eq!(binding.scope_id, None);
}

#[test]
fn ipv6_link_local_lease_binding_keeps_scope_id() {
    let selected = id("dddddddd-dddd-dddd-dddd-dddddddddddd");
    let mut link_local = addr("fe80::10", 64);
    link_local.scope_id = std::num::NonZeroU32::new(6);
    let info = adapter(selected, "Ethernet", true, false, true, vec![link_local]);
    let inv = NetworkInventory::new(vec![info], 3);
    let lease = inv
        .resolve(
            &AdapterSelection::Adapter(selected),
            Some("fe80::10".parse().unwrap()),
        )
        .unwrap();
    let binding = lease.network_binding();
    assert_eq!(binding.local_ip.to_string(), "fe80::10");
    assert_eq!(binding.scope_id, std::num::NonZeroU32::new(6));
    assert_eq!(binding.ipv6_if_index, std::num::NonZeroU32::new(2));
}

#[test]
fn synthetic_loopback_has_no_interface_index() {
    let loopback = synthetic_loopback();
    assert!(loopback.is_loopback);
    let binding = loopback.network_binding_for(loopback.addresses[0]);
    assert!(binding.is_loopback());
    assert_eq!(binding.ipv4_if_index, None);
    assert_eq!(binding.ipv6_if_index, None);
}
