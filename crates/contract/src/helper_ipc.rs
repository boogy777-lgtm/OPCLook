//! Versioned wire requests for the privileged route helper.
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    Hello {
        version: u32,
        client_pid: u32,
    },
    Add {
        version: u32,
        #[serde(flatten)]
        route: Route,
        lifetime_secs: u32,
    },
    Refresh {
        version: u32,
        #[serde(flatten)]
        route: Route,
        lifetime_secs: u32,
    },
    Remove {
        version: u32,
        #[serde(flatten)]
        route: Route,
    },
    Shutdown {
        version: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub target: IpAddr,
    pub prefix: u8,
    pub interface_luid: u64,
    pub next_hop: IpAddr,
}

impl Request {
    pub fn version(&self) -> u32 {
        match self {
            Self::Hello { version, .. }
            | Self::Add { version, .. }
            | Self::Refresh { version, .. }
            | Self::Remove { version, .. }
            | Self::Shutdown { version } => *version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_nulls_and_roundtrip() {
        let old = r#"{"version":2,"op":"hello","target":null,"prefix":null,"interface_luid":null,"next_hop":null,"lifetime_secs":null,"client_pid":123}"#;
        let request: Request = serde_json::from_str(old).unwrap();
        assert_eq!(
            request,
            Request::Hello {
                version: 2,
                client_pid: 123
            }
        );
        let route = Route {
            target: "10.0.0.1".parse().unwrap(),
            prefix: 32,
            interface_luid: 4,
            next_hop: "0.0.0.0".parse().unwrap(),
        };
        let old_add = r#"{"version":2,"op":"add","target":"10.0.0.1","prefix":32,"interface_luid":4,"next_hop":"0.0.0.0","lifetime_secs":3600,"client_pid":null}"#;
        assert_eq!(
            serde_json::from_str::<Request>(old_add).unwrap(),
            Request::Add {
                version: 2,
                route,
                lifetime_secs: 3600
            }
        );
        let old_remove = r#"{"version":2,"op":"remove","target":"10.0.0.1","prefix":32,"interface_luid":4,"next_hop":"0.0.0.0","lifetime_secs":null,"client_pid":null}"#;
        assert_eq!(
            serde_json::from_str::<Request>(old_remove).unwrap(),
            Request::Remove { version: 2, route }
        );
        for request in [
            Request::Hello {
                version: 2,
                client_pid: 123,
            },
            Request::Add {
                version: 2,
                route,
                lifetime_secs: 3600,
            },
            Request::Refresh {
                version: 2,
                route,
                lifetime_secs: 3600,
            },
            Request::Remove { version: 2, route },
            Request::Shutdown { version: 2 },
        ] {
            let bytes = serde_json::to_vec(&request).unwrap();
            assert_eq!(serde_json::from_slice::<Request>(&bytes).unwrap(), request);
        }
    }
    #[test]
    fn malformed_requests_are_rejected() {
        for json in [
            r#"{"version":2,"op":"unknown"}"#,
            r#"{"version":2,"op":"hello"}"#,
            r#"{"version":2,"op":"add","target":"bad","prefix":32,"interface_luid":1,"next_hop":"0.0.0.0","lifetime_secs":3}"#,
            r#"{"version":2,"op":"remove","target":"10.0.0.1"}"#,
        ] {
            assert!(serde_json::from_str::<Request>(json).is_err());
        }
    }
}
