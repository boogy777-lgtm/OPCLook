//! Validation and identity of OPC UA TCP endpoint URLs.
use std::net::{IpAddr, Ipv6Addr};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaUrl {
    original: String,
    host: String,
    port: u16,
    explicit_port: bool,
    path: String,
    query_fragment: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidUaUrl;

impl std::fmt::Display for InvalidUaUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid OPC UA endpoint URL")
    }
}
impl std::error::Error for InvalidUaUrl {}

impl FromStr for UaUrl {
    type Err = InvalidUaUrl;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.chars().any(char::is_control) {
            return Err(InvalidUaUrl);
        }
        let input = input.trim();
        let (scheme, rest) = input.split_once("://").ok_or(InvalidUaUrl)?;
        if !scheme.eq_ignore_ascii_case("opc.tcp") {
            return Err(InvalidUaUrl);
        }
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, suffix) = rest.split_at(end);
        if authority.is_empty()
            || authority.contains('@')
            || authority.chars().any(char::is_whitespace)
        {
            return Err(InvalidUaUrl);
        }
        let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
            let (host, tail) = inner.split_once(']').ok_or(InvalidUaUrl)?;
            host.parse::<Ipv6Addr>().map_err(|_| InvalidUaUrl)?;
            let port = if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(':').ok_or(InvalidUaUrl)?)
            };
            (host, port)
        } else {
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, None), |(h, p)| (h, Some(p)));
            if host.is_empty() || host.contains(['[', ']']) {
                return Err(InvalidUaUrl);
            }
            (host, port)
        };
        let explicit_port = port.is_some();
        let port = match port {
            Some(value) => value.parse::<u16>().map_err(|_| InvalidUaUrl)?,
            None => 4840,
        };
        if port == 0 {
            return Err(InvalidUaUrl);
        }
        let path_end = suffix.find(['?', '#']).unwrap_or(suffix.len());
        let (path, query_fragment) = suffix.split_at(path_end);
        Ok(Self {
            original: input.to_owned(),
            host: host.to_owned(),
            port,
            explicit_port,
            path: path.to_owned(),
            query_fragment: query_fragment.to_owned(),
        })
    }
}

impl UaUrl {
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn explicit_port(&self) -> bool {
        self.explicit_port
    }
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Hostnames compare without allocation, IP literals by address, and paths
    /// retain case. A trailing path slash remains compatible with older files.
    pub fn same_endpoint(&self, other: &Self) -> bool {
        let same_host = match (self.host.parse::<IpAddr>(), other.host.parse::<IpAddr>()) {
            (Ok(left), Ok(right)) => left == right,
            _ => self.host.eq_ignore_ascii_case(&other.host),
        };
        same_host
            && self.port == other.port
            && self.path.trim_end_matches('/') == other.path.trim_end_matches('/')
            && self.query_fragment == other.query_fragment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_and_identity_preserve_resources() {
        let a: UaUrl = "opc.tcp://[::1]/A?x=/".parse().unwrap();
        assert_eq!(
            (a.host(), a.port(), a.explicit_port()),
            ("::1", 4840, false)
        );
        assert!(a.same_endpoint(&"OPC.TCP://[0:0:0:0:0:0:0:1]:4840/A/?x=/".parse().unwrap()));
        assert!(!a.same_endpoint(&"opc.tcp://[::1]:4840/a?x=/".parse().unwrap()));
        assert!(!a.same_endpoint(&"opc.tcp://[::1]:4840/A?x=".parse().unwrap()));
        assert!(
            "opc.tcp://HOST:4850/X"
                .parse::<UaUrl>()
                .unwrap()
                .same_endpoint(&"opc.tcp://host:4850/X".parse().unwrap())
        );
        for bad in [
            "opc.tcp://user:secret@host:4840",
            "http://host",
            "opc.tcp://:4840",
            "opc.tcp://[::1",
            "opc.tcp://host:bad",
            "opc.tcp://host:",
            "opc.tcp://host:0",
            "opc.tcp://host/\npath",
            // The underlying SDK URL parser rejects IPv6 zone identifiers.
            "opc.tcp://[fe80::1%25eth0]:4840",
        ] {
            assert!(bad.parse::<UaUrl>().is_err(), "{bad}");
        }
    }
}
