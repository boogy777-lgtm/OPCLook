#[cfg(windows)]
use super::{AdapterAddress, AdapterId, AdapterState};
use super::{AdapterInfo, NetError};
#[cfg(windows)]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
#[cfg(windows)]
use std::num::NonZeroU32;

#[cfg(windows)]
pub fn inventory_adapters() -> Result<Vec<AdapterInfo>, NetError> {
    windows_inventory::read_adapters()
}

#[cfg(not(windows))]
pub fn inventory_adapters() -> Result<Vec<AdapterInfo>, NetError> {
    Err(NetError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_inventory {
    #![deny(clippy::undocumented_unsafe_blocks)]
    use super::{
        AdapterAddress, AdapterId, AdapterInfo, AdapterState, IpAddr, Ipv4Addr, Ipv6Addr, NetError,
        NonZeroU32,
    };
    use std::mem::size_of;
    use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS};
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_INCLUDE_PREFIX, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, GetIfEntry2,
        IF_TYPE_ETHERNET_CSMACD, IF_TYPE_GIGABITETHERNET, IF_TYPE_IEEE1394, IF_TYPE_IEEE80211,
        IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_WWANPP, IF_TYPE_WWANPP2, IP_ADAPTER_ADDRESSES_LH,
        MIB_IF_ROW2,
    };
    use windows::Win32::NetworkManagement::Ndis::{
        IfOperStatusDormant, IfOperStatusDown, IfOperStatusLowerLayerDown, IfOperStatusNotPresent,
        IfOperStatusTesting, IfOperStatusUnknown, IfOperStatusUp, NET_LUID_LH,
    };
    use windows::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6, SOCKET_ADDRESS,
    };

    pub fn read_adapters() -> Result<Vec<AdapterInfo>, NetError> {
        let flags = GAA_FLAG_INCLUDE_PREFIX
            | GAA_FLAG_INCLUDE_GATEWAYS
            | GAA_FLAG_SKIP_ANYCAST
            | GAA_FLAG_SKIP_MULTICAST
            | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size = 0u32;
        // SAFETY: the null buffer requests only the size in writable storage.
        let first = unsafe { GetAdaptersAddresses(0, flags, None, None, &mut size) };
        if first != ERROR_SUCCESS.0 && first != ERROR_BUFFER_OVERFLOW.0 {
            return Err(NetError::Inventory(format!(
                "GetAdaptersAddresses size query failed with {first}"
            )));
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let item = size_of::<IP_ADAPTER_ADDRESSES_LH>();
        let count = (size as usize + item - 1) / item;
        let mut buffer = vec![IP_ADAPTER_ADDRESSES_LH::default(); count];
        let mut size = (buffer.len() * item) as u32;
        // SAFETY: buffer is suitably aligned and contains size writable bytes.
        let result =
            unsafe { GetAdaptersAddresses(0, flags, None, Some(buffer.as_mut_ptr()), &mut size) };
        if result == ERROR_NO_DATA.0 {
            return Ok(Vec::new());
        }
        if result != ERROR_SUCCESS.0 {
            return Err(NetError::Inventory(format!(
                "GetAdaptersAddresses failed with {result}"
            )));
        }

        let mut adapters = Vec::new();
        let mut current = buffer.as_ptr();
        while !current.is_null() {
            // SAFETY: the successful API call returned this live linked entry.
            let raw = unsafe { &*current };
            // SAFETY: all nested pointers refer to the live API output buffer.
            adapters.push(unsafe { from_raw_adapter(raw) });
            current = raw.Next;
        }
        adapters.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(adapters)
    }

    unsafe fn from_raw_adapter(raw: &IP_ADAPTER_ADDRESSES_LH) -> AdapterInfo {
        // SAFETY: adapter names are nullable terminated strings in the API buffer.
        let name = unsafe { crate::winutil::wide_to_string(raw.FriendlyName) };
        // SAFETY: the description is a nullable terminated API string.
        let description = unsafe { crate::winutil::wide_to_string(raw.Description) };
        let mut addresses = Vec::new();
        let mut unicast = raw.FirstUnicastAddress;
        while !unicast.is_null() {
            // SAFETY: the API supplies this linked unicast entry in its live buffer.
            let entry = unsafe { &*unicast };
            // SAFETY: this socket address belongs to the live API output buffer.
            if let Some((ip, scope_id)) = unsafe { socket_ip(entry.Address) } {
                addresses.push(AdapterAddress {
                    ip,
                    prefix: entry.OnLinkPrefixLength,
                    scope_id,
                });
            }
            unicast = entry.Next;
        }
        let mut gateways = Vec::new();
        let mut gateway = raw.FirstGatewayAddress;
        while !gateway.is_null() {
            // SAFETY: the API supplies this linked gateway entry in its live buffer.
            let entry = unsafe { &*gateway };
            // SAFETY: the address belongs to the live API output buffer.
            if let Some((ip, _)) = unsafe { socket_ip(entry.Address) } {
                gateways.push(ip);
            }
            gateway = entry.Next;
        }
        // SAFETY: GetAdaptersAddresses initialized the adapter index union.
        let if_index = NonZeroU32::new(unsafe { raw.Anonymous1.Anonymous.IfIndex });
        let ipv6_if_index = NonZeroU32::new(raw.Ipv6IfIndex);
        // SAFETY: the initialized LUID union is read as its integer representation.
        let luid = unsafe { raw.Luid.Value };
        let state = map_oper_status(raw.OperStatus);
        let is_loopback = raw.IfType == IF_TYPE_SOFTWARE_LOOPBACK
            || (addresses.iter().all(|a| a.ip.is_loopback()) && !addresses.is_empty())
            || looks_like_loopback(&name, &description);
        let is_physical =
            !is_loopback && physical_if_type(raw.IfType) && !looks_virtual(&name, &description);
        // `NetworkGuid` is the network-profile identity, not the adapter
        // identity. It can be shared by several adapters (and produced the
        // duplicate-GUID cache that hid every adapter). `AdapterName` is the
        // adapter connection GUID, which is stable and unique.
        let id = if raw.AdapterName.is_null() {
            AdapterId::UNSPECIFIED
        } else {
            // SAFETY: a non-null adapter name is terminated in the API buffer.
            unsafe { raw.AdapterName.to_string() }
                .ok()
                .and_then(|name| AdapterId::parse(&name).ok())
                .unwrap_or(AdapterId::UNSPECIFIED)
        };
        let mut info = AdapterInfo {
            id,
            name,
            description,
            state,
            if_type: raw.IfType,
            addresses,
            gateways,
            if_index,
            ipv6_if_index,
            luid,
            metric: effective_metric(raw.Ipv4Metric, raw.Ipv6Metric),
            is_physical,
            is_loopback,
        };
        enrich_from_if_entry(&mut info);
        info
    }

    unsafe fn socket_ip(address: SOCKET_ADDRESS) -> Option<(IpAddr, Option<NonZeroU32>)> {
        if address.lpSockaddr.is_null() {
            return None;
        }
        if address.iSockaddrLength
            < size_of::<windows::Win32::Networking::WinSock::ADDRESS_FAMILY>() as i32
        {
            return None;
        }
        // SAFETY: the API supplied an aligned address with room for its family.
        let family = unsafe { (*address.lpSockaddr).sa_family };
        if family == AF_INET {
            if address.iSockaddrLength < size_of::<SOCKADDR_IN>() as i32 {
                return None;
            }
            // SAFETY: family and length identify a complete aligned IPv4 sockaddr.
            let sin = unsafe { &*(address.lpSockaddr as *const SOCKADDR_IN) };
            // SAFETY: an IPv4 sockaddr initializes this address union member.
            let raw = unsafe { sin.sin_addr.S_un.S_addr };
            let ip = Ipv4Addr::from(u32::from_be(raw));
            Some((IpAddr::V4(ip), None))
        } else if family == AF_INET6 {
            if address.iSockaddrLength < size_of::<SOCKADDR_IN6>() as i32 {
                return None;
            }
            // SAFETY: family and length identify a complete aligned IPv6 sockaddr.
            let sin6 = unsafe { &*(address.lpSockaddr as *const SOCKADDR_IN6) };
            // SAFETY: an IPv6 sockaddr initializes its address union.
            let bytes = unsafe { sin6.sin6_addr.u.Byte };
            // SAFETY: the scope union is initialized by the socket address API.
            let scope = unsafe { sin6.Anonymous.sin6_scope_id };
            Some((IpAddr::V6(Ipv6Addr::from(bytes)), NonZeroU32::new(scope)))
        } else {
            None
        }
    }

    fn enrich_from_if_entry(info: &mut AdapterInfo) {
        let mut row = MIB_IF_ROW2::default();
        row.InterfaceLuid = NET_LUID_LH { Value: info.luid };
        // SAFETY: row contains the selected LUID and is writable for the API.
        let result = unsafe { GetIfEntry2(&mut row) };
        if result == ERROR_SUCCESS {
            info.state = map_oper_status(row.OperStatus);
        }
    }

    fn map_oper_status(
        status: windows::Win32::NetworkManagement::Ndis::IF_OPER_STATUS,
    ) -> AdapterState {
        if status == IfOperStatusUp {
            AdapterState::Up
        } else if status == IfOperStatusDown {
            AdapterState::Down
        } else if status == IfOperStatusTesting {
            AdapterState::Testing
        } else if status == IfOperStatusDormant {
            AdapterState::Dormant
        } else if status == IfOperStatusNotPresent {
            AdapterState::NotPresent
        } else if status == IfOperStatusLowerLayerDown {
            AdapterState::LowerLayerDown
        } else if status == IfOperStatusUnknown {
            AdapterState::Unknown
        } else {
            AdapterState::Unknown
        }
    }

    fn effective_metric(v4: u32, v6: u32) -> u32 {
        match (v4, v6) {
            (0, 0) => 0,
            (0, v6) => v6,
            (v4, 0) => v4,
            (v4, v6) => v4.min(v6),
        }
    }

    fn physical_if_type(if_type: u32) -> bool {
        matches!(
            if_type,
            IF_TYPE_ETHERNET_CSMACD
                | IF_TYPE_GIGABITETHERNET
                | IF_TYPE_IEEE80211
                | IF_TYPE_IEEE1394
                | IF_TYPE_WWANPP
                | IF_TYPE_WWANPP2
        )
    }

    fn looks_virtual(name: &str, description: &str) -> bool {
        const VIRTUAL_MARKERS: &[&str] = &[
            "virtual",
            "vpn",
            "tap",
            "tun",
            "wireguard",
            "openvpn",
            "hyper-v",
            "vmware",
            "virtualbox",
            "loopback",
            "pseudo",
            "docker",
            "wsl",
            "tailscale",
            "zerotier",
            "npcap",
        ];
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        VIRTUAL_MARKERS
            .iter()
            .any(|marker| haystack.contains(marker))
    }

    fn looks_like_loopback(name: &str, description: &str) -> bool {
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        haystack.contains("loopback")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn null_adapter_name_and_short_socket_address_are_safe() {
            let raw = IP_ADAPTER_ADDRESSES_LH::default();
            // SAFETY: all nullable fields of the initialized empty structure are null.
            let info = unsafe { from_raw_adapter(&raw) };
            assert_eq!(info.id, AdapterId::UNSPECIFIED);
            let mut address = SOCKADDR_IN::default();
            let socket = SOCKET_ADDRESS {
                lpSockaddr: (&raw mut address).cast(),
                iSockaddrLength: 0,
            };
            // SAFETY: the pointer refers to live aligned storage; length is intentionally short.
            assert!(unsafe { socket_ip(socket) }.is_none());
        }

        #[test]
        fn physical_and_virtual_interface_classification() {
            assert!(physical_if_type(IF_TYPE_ETHERNET_CSMACD));
            assert!(physical_if_type(IF_TYPE_IEEE80211));
            assert!(!physical_if_type(IF_TYPE_SOFTWARE_LOOPBACK));
            assert!(!physical_if_type(
                windows::Win32::NetworkManagement::IpHelper::IF_TYPE_TUNNEL
            ));

            assert!(looks_virtual(
                "vEthernet (Default Switch)",
                "Hyper-V Virtual Ethernet Adapter"
            ));
            assert!(looks_virtual("Ethernet 3", "TAP-Windows Adapter V9"));
            assert!(looks_virtual("VPN", "WireGuard Tunnel"));
            assert!(!looks_virtual(
                "Ethernet",
                "Intel(R) Ethernet Connection I219-LM"
            ));
            assert!(!looks_virtual("Wi-Fi", "Intel(R) Wi-Fi 6 AX201 160MHz"));

            assert!(looks_like_loopback("Loopback Pseudo-Interface 1", ""));
            assert!(!looks_like_loopback("Ethernet", "Intel Ethernet"));
        }

        #[test]
        fn effective_metric_ignores_zero_entries() {
            assert_eq!(effective_metric(0, 0), 0);
            assert_eq!(effective_metric(0, 25), 25);
            assert_eq!(effective_metric(10, 0), 10);
            assert_eq!(effective_metric(25, 5), 5);
        }
    }
}
