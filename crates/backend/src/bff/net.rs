//! UA transport binding to a selected network adapter.
//!
//! One `NetworkBinding` is an immutable snapshot (local IP, IPv4/IPv6 interface
//! indexes and IPv6 link-local scope). `BoundTcpConnector` creates a fresh TCP
//! socket for every `connect()` call and pins it before connecting:
//!
//! ```text
//! socket -> optional IP_UNICAST_IF / IPV6_UNICAST_IF -> bind(local:0) -> connect
//! ```
//!
//! Loopback destinations always use a loopback source address and never apply
//! an interface option. No socket is shared between sessions or discovery
//! requests.

use std::io;
#[cfg(test)]
use std::net::Ipv4Addr;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use opcua::client::ConnectionSource;
use opcua::client::transport::{
    Connector, ConnectorBuilder, OutgoingMessage, SecureChannelState, StreamConnection,
    StreamConnector, TcpTransport, TransportConfiguration,
};
use opcua::core::comms::tcp_codec::TcpCodec;
use opcua::core::comms::url::hostname_port_from_url;
use opcua::core::constants::DEFAULT_OPC_UA_SERVER_PORT;
use opcua::types::{DecodingOptions, EndpointDescription, Error, StatusCode};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::{TcpSocket, TcpStream};
use tokio_util::codec::FramedRead;

pub use opclook_contract::net::NetworkBinding;

/// URL builder that brackets IPv6 literals (`opc.tcp://[::1]:4840`).
pub(crate) fn endpoint_url_for(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(ip) => format!("opc.tcp://{ip}:{port}"),
        IpAddr::V6(ip) => format!("opc.tcp://[{ip}]:{port}"),
    }
}

/// Parse an `opc.tcp://` URL into a resolver host (brackets stripped) and port.
pub(crate) fn parse_opc_tcp_url(url: &str) -> Result<(String, u16), Error> {
    let (raw_host, port) =
        hostname_port_from_url(url, DEFAULT_OPC_UA_SERVER_PORT).map_err(|_| {
            Error::new(
                StatusCode::BadTcpEndpointUrlInvalid,
                format!("invalid OPC UA URL: {url}"),
            )
        })?;
    let host = raw_host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    if host.is_empty() {
        return Err(Error::new(
            StatusCode::BadTcpEndpointUrlInvalid,
            format!("OPC UA URL has no host: {url}"),
        ));
    }
    Ok((host, port))
}

/// Connector that pins a fresh TCP socket to `binding` for every connection.
#[derive(Clone, Debug)]
pub struct BoundTcpConnector {
    endpoint_url: String,
    binding: NetworkBinding,
}

impl BoundTcpConnector {
    pub fn new(endpoint_url: impl Into<String>, binding: NetworkBinding) -> Result<Self, Error> {
        let endpoint_url = endpoint_url.into();
        parse_opc_tcp_url(&endpoint_url)?;
        Ok(Self {
            endpoint_url,
            binding,
        })
    }

    pub fn endpoint_url(&self) -> &str {
        &self.endpoint_url
    }

    pub fn binding(&self) -> &NetworkBinding {
        &self.binding
    }
}

/// Builder for [`BoundTcpConnector`] from an endpoint URL.
#[derive(Clone, Debug)]
pub struct BoundConnectorBuilder {
    endpoint_url: String,
    binding: NetworkBinding,
}

impl BoundConnectorBuilder {
    pub fn new(endpoint_url: impl Into<String>, binding: NetworkBinding) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
            binding,
        }
    }
}

impl ConnectorBuilder for BoundConnectorBuilder {
    type ConnectorType = BoundTcpConnector;

    fn build(self) -> Result<Self::ConnectorType, Error> {
        BoundTcpConnector::new(self.endpoint_url, self.binding)
    }
}

/// Connection source for `SessionBuilder::with_connector`.
#[derive(Clone, Debug)]
pub struct BoundConnectionSource {
    binding: NetworkBinding,
}

impl BoundConnectionSource {
    pub fn new(binding: NetworkBinding) -> Self {
        Self { binding }
    }

    pub fn binding(&self) -> &NetworkBinding {
        &self.binding
    }
}

impl ConnectionSource for BoundConnectionSource {
    type Builder = BoundConnectorBuilder;

    fn get_connector(&self, endpoint: &EndpointDescription) -> Result<Self::Builder, Error> {
        Ok(BoundConnectorBuilder::new(
            endpoint.endpoint_url.as_ref(),
            self.binding.clone(),
        ))
    }
}

impl Connector for BoundTcpConnector {
    type Transport = TcpTransport;

    async fn connect(
        &self,
        channel: Arc<SecureChannelState>,
        outgoing_recv: tokio::sync::mpsc::Receiver<OutgoingMessage>,
        config: TransportConfiguration,
    ) -> Result<Self::Transport, Error> {
        let binding = self.binding.clone();
        let inner = StreamConnector::new(
            move |endpoint_url: String, options: DecodingOptions| {
                let binding = binding.clone();
                async move { connect_bound_tcp(endpoint_url, binding, options).await }
            },
            self.endpoint_url.clone(),
        );
        inner.connect(channel, outgoing_recv, config).await
    }

    fn default_endpoint(&self) -> EndpointDescription {
        EndpointDescription::from(self.endpoint_url.as_str())
    }
}

async fn connect_bound_tcp(
    endpoint_url: String,
    binding: NetworkBinding,
    decoding_options: DecodingOptions,
) -> Result<StreamConnection<ReadHalf<TcpStream>, WriteHalf<TcpStream>>, Error> {
    let (host, port) = parse_opc_tcp_url(&endpoint_url)?;
    let remotes = resolve_remote(&host, port, &endpoint_url).await?;

    let mut candidates = Vec::new();
    for remote in remotes {
        // Skip addresses of the other family instead of retrying them through
        // an unbound socket. Loopback always works regardless of the selected
        // card; connect_one switches its source to loopback.
        if !remote.ip().is_loopback() && remote.is_ipv4() != binding.local_ip.is_ipv4() {
            continue;
        }
        if !candidates.contains(&remote) {
            candidates.push(remote);
        }
    }
    if candidates.is_empty() {
        return Err(Error::new(
            StatusCode::BadTcpEndpointUrlInvalid,
            format!(
                "no resolved address for {endpoint_url} matches the bound {} source",
                binding.local_ip
            ),
        ));
    }
    // One attempt only: prefer IPv4 for ambiguous loopback names, then use the
    // resolver order. Higher layers (discovery search) own any address retry.
    candidates.sort_by_key(|address| address.is_ipv6());
    connect_one(candidates[0], &binding, &decoding_options, &endpoint_url).await
}

async fn resolve_remote(
    host: &str,
    port: u16,
    endpoint_url: &str,
) -> Result<Vec<SocketAddr>, Error> {
    if let Some(ip) = parse_ip_literal(host, port) {
        return Ok(vec![ip]);
    }
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            Error::new(
                StatusCode::BadTcpEndpointUrlInvalid,
                format!("could not resolve {host}:{port} for {endpoint_url}: {error}"),
            )
        })?;
    let mut out = Vec::new();
    for address in resolved {
        if !out.contains(&address) {
            out.push(address);
        }
    }
    if out.is_empty() {
        return Err(Error::new(
            StatusCode::BadTcpEndpointUrlInvalid,
            format!("{host}:{port} resolved to no addresses for {endpoint_url}"),
        ));
    }
    Ok(out)
}

fn parse_ip_literal(host: &str, port: u16) -> Option<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    // IPv6 link-local URL form: fe80::1%12.
    if let Some((address, scope)) = host.rsplit_once('%') {
        if let (Ok(ip), Ok(scope)) = (address.parse::<Ipv6Addr>(), scope.parse::<u32>()) {
            return Some(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, scope)));
        }
    }
    None
}

async fn connect_one(
    remote: SocketAddr,
    binding: &NetworkBinding,
    decoding_options: &DecodingOptions,
    endpoint_url: &str,
) -> Result<StreamConnection<ReadHalf<TcpStream>, WriteHalf<TcpStream>>, Error> {
    let stream = open_bound_socket(remote, binding).await?;
    let (reader, writer) = tokio::io::split(stream);
    Ok(StreamConnection::new(
        FramedRead::new(reader, TcpCodec::new(decoding_options.clone())),
        writer,
        endpoint_url.to_string(),
    ))
}

/// Cheap TCP-only precheck used by the subnet scanner before it spends a UA
/// handshake on an address. The same bound socket path is used as for real
/// traffic, so the probe goes through the selected adapter.
pub(crate) async fn tcp_probe(
    remote: SocketAddr,
    binding: &NetworkBinding,
    timeout: Duration,
) -> bool {
    matches!(
        tokio::time::timeout(timeout, open_bound_socket(remote, binding)).await,
        Ok(Ok(_))
    )
}

async fn open_bound_socket(
    remote: SocketAddr,
    binding: &NetworkBinding,
) -> Result<TcpStream, Error> {
    let loopback = remote.ip().is_loopback();
    let effective = if loopback {
        NetworkBinding::loopback_for(remote.ip())
    } else {
        binding.clone()
    };
    let remote = effective.prepare_remote(remote);

    let socket = match remote {
        SocketAddr::V4(_) => TcpSocket::new_v4(),
        SocketAddr::V6(_) => TcpSocket::new_v6(),
    }
    .map_err(|error| {
        Error::new(
            StatusCode::BadCommunicationError,
            format!("could not create TCP socket: {error}"),
        )
    })?;

    // Loopback traffic is never pinned to a physical interface.
    if !(loopback || effective.local_ip.is_loopback()) {
        if let Some(index) = effective.interface_index_for(remote.ip()) {
            apply_interface_option(&socket, remote.ip(), index).map_err(|error| {
                Error::new(
                    StatusCode::BadCommunicationError,
                    format!(
                        "could not bind interface index {index} for {}: {error}",
                        remote.ip()
                    ),
                )
            })?;
        }
    }

    if let Some(local) = effective.local_socket_addr(remote.ip()) {
        socket.bind(local).map_err(|error| {
            Error::new(
                StatusCode::BadCommunicationError,
                format!("could not bind local address {local}: {error}"),
            )
        })?;
    }

    socket.connect(remote).await.map_err(|error| {
        Error::new(
            StatusCode::BadCommunicationError,
            format!("could not connect to {remote}: {error}"),
        )
    })
}

#[cfg(windows)]
fn apply_interface_option(socket: &TcpSocket, remote: IpAddr, index: u32) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Networking::WinSock::{
        IP_UNICAST_IF, IPPROTO_IP, IPPROTO_IPV6, IPV6_UNICAST_IF, SOCKET, WSAGetLastError,
        setsockopt,
    };

    let (level, option) = match remote {
        IpAddr::V4(_) => (IPPROTO_IP.0, IP_UNICAST_IF),
        IpAddr::V6(_) => (IPPROTO_IPV6.0, IPV6_UNICAST_IF),
    };
    // The Winsock option takes the interface index in network byte order.
    let value = index.to_be_bytes();
    let raw = socket.as_raw_socket();
    let result = unsafe { setsockopt(SOCKET(raw as usize), level, option, Some(&value)) };
    if result != 0 {
        let code = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(code.0));
    }
    Ok(())
}

#[cfg(not(windows))]
fn apply_interface_option(_socket: &TcpSocket, _remote: IpAddr, _index: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "interface binding is only supported on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_binding_has_no_interface_option() {
        let v4 = NetworkBinding::loopback();
        assert!(v4.is_loopback());
        assert_eq!(v4.local_ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(
            v4.interface_index_for(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            None
        );

        let v6 = NetworkBinding::loopback_for(IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(v6.local_ip, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(
            v6.interface_index_for(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            None
        );
        assert_eq!(
            v6.local_socket_addr(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0))
        );
    }

    #[test]
    fn ipv4_and_ipv6_bindings_select_the_right_family_and_index() {
        let binding =
            NetworkBinding::new("192.168.1.20".parse().unwrap(), Some(12), Some(24), None);
        assert_eq!(
            binding.interface_index_for("10.0.0.1".parse().unwrap()),
            Some(12)
        );
        assert_eq!(
            binding.interface_index_for("2001:db8::1".parse().unwrap()),
            Some(24)
        );
        assert_eq!(
            binding.local_socket_addr("8.8.8.8".parse().unwrap()),
            Some("192.168.1.20:0".parse().unwrap())
        );
        assert_eq!(
            binding.local_socket_addr("2001:db8::1".parse().unwrap()),
            None
        );
    }

    #[test]
    fn ipv6_link_local_binding_keeps_scope_id() {
        let binding = NetworkBinding::new("fe80::1".parse().unwrap(), Some(12), Some(24), Some(7));
        assert_eq!(binding.scope_id, Some(7));
        assert_eq!(
            binding.local_socket_addr("fe80::2".parse().unwrap()),
            Some("[fe80::1%7]:0".parse().unwrap())
        );
        assert_eq!(
            binding.prepare_remote("[fe80::2]:4840".parse().unwrap()),
            "[fe80::2%7]:4840".parse().unwrap()
        );
    }

    #[test]
    fn invalid_urls_are_rejected_without_panicking() {
        for url in [
            "",
            "http://127.0.0.1:4840",
            "opc.tcp://",
            "opc.tcp://:4840",
            "opc.tcp://[::1",
        ] {
            assert!(
                BoundTcpConnector::new(url, NetworkBinding::loopback()).is_err(),
                "accepted {url:?}"
            );
        }
    }

    #[test]
    fn ipv6_url_is_bracketed_before_resolution() {
        assert_eq!(
            endpoint_url_for("::1".parse().unwrap(), 4840),
            "opc.tcp://[::1]:4840"
        );
        let (host, port) = parse_opc_tcp_url("opc.tcp://[::1]:4840").unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 4840);
        assert_eq!(
            parse_ip_literal(&host, port).unwrap(),
            "[::1]:4840".parse().unwrap()
        );
    }

    #[test]
    fn ipv6_url_without_explicit_port_uses_default() {
        let (host, port) = parse_opc_tcp_url("opc.tcp://[::1]").unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 4840);
    }

    #[test]
    fn loopback_connect_ignores_a_physical_binding() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { listener.accept().await.unwrap().1 });
            let physical =
                NetworkBinding::new("192.0.2.10".parse().unwrap(), Some(123), Some(124), None);
            let connection = connect_bound_tcp(
                format!("opc.tcp://127.0.0.1:{}", address.port()),
                physical,
                DecodingOptions::default(),
            )
            .await;
            let _connection = match connection {
                Ok(connection) => connection,
                Err(error) => panic!("loopback connect failed: {error}"),
            };
            let peer = server.await.unwrap();
            assert_eq!(peer.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert_ne!(peer.port(), 0);
        });
    }

    #[test]
    fn bound_builder_builds_a_connector() {
        let builder =
            BoundConnectorBuilder::new("opc.tcp://127.0.0.1:4840", NetworkBinding::loopback());
        let connector = builder.build().unwrap();
        assert_eq!(connector.endpoint_url(), "opc.tcp://127.0.0.1:4840");
        assert!(connector.binding().is_loopback());
    }
}
