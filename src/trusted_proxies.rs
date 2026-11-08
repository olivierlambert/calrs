//! Which TCP peers may tell calrs who the client is, through
//! `X-Forwarded-For` (#85).
//!
//! The per-address rate limits (sign-in, two-factor codes, bookings, OAuth)
//! need the client's address. Behind a reverse proxy the TCP peer is the
//! proxy, and the client is in `X-Forwarded-For`; exposed directly, the peer
//! is the client and any `X-Forwarded-For` is whatever the client chose to
//! send. So the header is believed only when the peer is a trusted proxy,
//! and only back to the first hop that is not one.
//!
//! The list comes from `CALRS_TRUSTED_PROXIES` when set, else from the admin
//! System settings (`auth_config.trusted_proxies`), else the default: the
//! loopback, private (RFC 1918), unique local (`fc00::/7`) and link-local
//! ranges, where a proxy on the same host or Docker network lives. `none`
//! trusts no peer.

use std::net::IpAddr;
use std::sync::{OnceLock, RwLock};

/// One address range, `addr/prefix`. A bare address is a single host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(raw: &str) -> Option<Cidr> {
        let raw = raw.trim();
        let (addr, prefix) = match raw.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (raw, None),
        };
        let addr: IpAddr = addr.trim().parse().ok()?;
        let addr = addr.to_canonical();
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.trim().parse::<u8>().ok().filter(|p| *p <= max)?,
            None => max,
        };
        Some(Cidr { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip.to_canonical()) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for Cidr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

/// The ranges used when nothing is configured.
pub const DEFAULT: &[&str] = &[
    "127.0.0.0/8",
    "::1/128",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "fc00::/7",
    "169.254.0.0/16",
    "fe80::/10",
];

/// A list of trusted ranges. Empty trusts no peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedProxies(Vec<Cidr>);

impl TrustedProxies {
    pub fn default_ranges() -> Self {
        TrustedProxies(DEFAULT.iter().filter_map(|r| Cidr::parse(r)).collect())
    }

    pub fn none() -> Self {
        TrustedProxies(Vec::new())
    }

    /// Parses a comma or whitespace separated list of addresses and CIDR
    /// ranges, or `none`. An empty value is the default list. A `/0` range
    /// (`0.0.0.0/0`, `::/0`) is refused: it would trust every peer, which is
    /// the forged-address hole this list exists to close.
    pub fn parse(raw: &str) -> Result<Self, ListError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(Self::default_ranges());
        }
        if raw.eq_ignore_ascii_case("none") {
            return Ok(Self::none());
        }
        raw.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|entry| !entry.is_empty())
            .map(|entry| match Cidr::parse(entry) {
                None => Err(ListError::Invalid(entry.to_string())),
                Some(range) if range.prefix == 0 => {
                    Err(ListError::TrustsEveryone(entry.to_string()))
                }
                Some(range) => Ok(range),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(TrustedProxies)
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|range| range.contains(ip))
    }

    /// The list as it is written back to the admin form.
    pub fn describe(&self) -> String {
        if self.0.is_empty() {
            return "none".to_string();
        }
        self.0
            .iter()
            .map(Cidr::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Why a trusted proxy list was refused, naming the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    /// Neither an address nor a CIDR range.
    Invalid(String),
    /// A `/0` range, which trusts every peer.
    TrustsEveryone(String),
}

impl ListError {
    pub fn entry(&self) -> &str {
        match self {
            ListError::Invalid(entry) | ListError::TrustsEveryone(entry) => entry,
        }
    }
}

impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListError::Invalid(entry) => write!(f, "{entry:?} is not an address or CIDR range"),
            ListError::TrustsEveryone(entry) => write!(f, "{entry:?} would trust every peer"),
        }
    }
}

static FROM_ENV: OnceLock<Option<TrustedProxies>> = OnceLock::new();
static FROM_DB: RwLock<Option<TrustedProxies>> = RwLock::new(None);

/// `CALRS_TRUSTED_PROXIES`, read once. An unusable value trusts no peer
/// (and says so in the log): failing open would hand every client a forged
/// address.
fn from_env() -> Option<&'static TrustedProxies> {
    FROM_ENV
        .get_or_init(|| {
            let raw = std::env::var("CALRS_TRUSTED_PROXIES").ok()?;
            Some(TrustedProxies::parse(&raw).unwrap_or_else(|e| {
                tracing::error!(
                    error = %e,
                    "CALRS_TRUSTED_PROXIES refused; trusting no proxy"
                );
                TrustedProxies::none()
            }))
        })
        .as_ref()
}

/// Whether `CALRS_TRUSTED_PROXIES` is forcing the value.
pub fn set_by_env() -> bool {
    from_env().is_some()
}

/// The list in force: the environment's, else the saved one, else the
/// default.
pub fn current() -> TrustedProxies {
    if let Some(list) = from_env() {
        return list.clone();
    }
    FROM_DB
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(TrustedProxies::default_ranges)
}

/// The saved value as the admin form shows it, empty when none is saved.
pub fn saved() -> String {
    FROM_DB
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(TrustedProxies::describe))
        .unwrap_or_default()
}

/// Caches the saved value (`auth_config.trusted_proxies`). NULL or empty
/// means the default; a value the admin form would refuse is treated like a
/// bad environment value.
pub fn set_from_db(raw: Option<&str>) {
    let list = raw.filter(|r| !r.trim().is_empty()).map(|r| {
        TrustedProxies::parse(r).unwrap_or_else(|e| {
            tracing::error!(error = %e, "saved trusted proxies refused; trusting no proxy");
            TrustedProxies::none()
        })
    });
    if let Ok(mut g) = FROM_DB.write() {
        *g = list;
    }
}

/// The client address a request names, given the TCP peer and the
/// `X-Forwarded-For` header lines in the order they arrived.
///
/// - No peer (a server not run with connection info): the header is ignored
///   and the address is unknown. `calrs serve` always has one.
/// - An untrusted peer is the client, whatever the header says.
/// - A trusted peer's header is read as one list, every line joined in
///   order (HAProxy appends a line of its own rather than extending the
///   client's), right to left past every trusted hop. The first address
///   that is not trusted is the client; all trusted, the leftmost.
/// - A hop that is not an address (not UTF-8, `unknown`, a hostname) stops
///   the walk: the client is then [`ClientAddress::Unparseable`], one bucket
///   for every such request, never the proxy's own address, so garbage
///   cannot be used to fill the bucket everyone behind the proxy shares.
/// - A trusted peer with no header is itself the address, shared by every
///   client behind it.
pub fn client_address(
    peer: Option<IpAddr>,
    forwarded_for: &[&[u8]],
    trusted: &TrustedProxies,
) -> ClientAddress {
    let Some(peer) = peer.map(|p| p.to_canonical()) else {
        return ClientAddress::Unknown;
    };
    if !trusted.contains(peer) {
        return ClientAddress::Direct(peer.to_string());
    }
    let hops: Vec<&[u8]> = forwarded_for
        .iter()
        .flat_map(|line| line.split(|b| *b == b','))
        .map(|hop| hop.trim_ascii())
        .filter(|hop| !hop.is_empty())
        .collect();
    let mut leftmost_trusted = None;
    for hop in hops.iter().rev() {
        match parse_hop(hop) {
            Some(ip) if trusted.contains(ip) => leftmost_trusted = Some(ip),
            Some(ip) => return ClientAddress::Forwarded(ip.to_string()),
            None => return ClientAddress::Unparseable,
        }
    }
    match leftmost_trusted {
        Some(ip) => ClientAddress::Forwarded(ip.to_string()),
        None => ClientAddress::Proxy(peer.to_string()),
    }
}

/// One `X-Forwarded-For` hop as an address: a bare IPv4 or IPv6 address,
/// with an optional port (`1.2.3.4:5678`, `[2001:db8::1]:443`, as Azure
/// Application Gateway and IIS ARR write them) or brackets dropped, so a
/// port cannot make a new key. Anything else is `None`.
fn parse_hop(hop: &[u8]) -> Option<IpAddr> {
    let hop = std::str::from_utf8(hop).ok()?;
    if let Ok(ip) = hop.parse::<IpAddr>() {
        return Some(ip.to_canonical());
    }
    if let Some(rest) = hop.strip_prefix('[') {
        let (inner, after) = rest.split_once(']')?;
        let port_ok = after.is_empty()
            || after
                .strip_prefix(':')
                .is_some_and(|p| p.parse::<u16>().is_ok());
        return port_ok
            .then(|| inner.parse::<std::net::Ipv6Addr>().ok())
            .flatten()
            .map(|ip| IpAddr::V6(ip).to_canonical());
    }
    let (addr, port) = hop.rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    addr.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4)
}

/// Where a request's client address came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientAddress {
    /// An untrusted TCP peer: the client itself.
    Direct(String),
    /// Named by a trusted proxy in `X-Forwarded-For`.
    Forwarded(String),
    /// A trusted proxy that named no client: every client behind it shares
    /// this address.
    Proxy(String),
    /// A trusted proxy forwarded a hop that is not an address: one shared
    /// bucket for all such requests.
    Unparseable,
    /// No peer address.
    Unknown,
}

impl ClientAddress {
    /// The address, `"unparseable"` or `"unknown"` when there is none.
    pub fn ip(&self) -> &str {
        match self {
            ClientAddress::Direct(ip) | ClientAddress::Forwarded(ip) | ClientAddress::Proxy(ip) => {
                ip
            }
            ClientAddress::Unparseable => "unparseable",
            ClientAddress::Unknown => "unknown",
        }
    }

    /// Whether the address is one client's rather than one shared by every
    /// client behind a proxy (or unparseable, or unknown).
    pub fn is_individual(&self) -> bool {
        matches!(self, ClientAddress::Direct(_) | ClientAddress::Forwarded(_))
    }
}

/// The rate-limit identity of an address: an IPv6 address counts by its /64,
/// which one subscriber usually holds whole, so rotating addresses inside it
/// buys neither a new budget nor a new row. IPv4 (and IPv4-mapped IPv6)
/// counts per address; anything else is kept as it is.
pub fn rate_key(ip: &str) -> String {
    match ip.parse::<IpAddr>().map(|ip| ip.to_canonical()) {
        Ok(IpAddr::V6(v6)) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Ok(IpAddr::V4(v4)) => v4.to_string(),
        Err(_) => ip.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn cidr_matching() {
        let net = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(net.contains(ip("10.255.0.1")));
        assert!(!net.contains(ip("11.0.0.1")));
        assert!(net.contains(ip("::ffff:10.1.2.3")), "IPv4-mapped peer");
        let ula = Cidr::parse("fc00::/7").unwrap();
        assert!(ula.contains(ip("fd12:3456::1")));
        assert!(!ula.contains(ip("2001:db8::1")));
        assert!(Cidr::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
        assert!(Cidr::parse("203.0.113.5")
            .unwrap()
            .contains(ip("203.0.113.5")));
        assert!(!Cidr::parse("203.0.113.5")
            .unwrap()
            .contains(ip("203.0.113.6")));
        assert!(Cidr::parse("10.0.0.0/33").is_none());
        assert!(Cidr::parse("proxy.local").is_none());
    }

    #[test]
    fn parse_lists() {
        assert_eq!(
            TrustedProxies::parse("").unwrap(),
            TrustedProxies::default_ranges()
        );
        assert_eq!(
            TrustedProxies::parse(" NONE ").unwrap(),
            TrustedProxies::none()
        );
        let list = TrustedProxies::parse("203.0.113.5, 2001:db8::/32 10.1.0.0/16").unwrap();
        assert!(list.contains(ip("2001:db8:1::1")));
        assert!(!list.contains(ip("192.168.1.1")));
        assert_eq!(
            list.describe(),
            "203.0.113.5/32, 2001:db8::/32, 10.1.0.0/16"
        );
        assert_eq!(
            TrustedProxies::parse("10.0.0.1, nope"),
            Err(ListError::Invalid("nope".to_string()))
        );
        let default = TrustedProxies::default_ranges();
        for addr in [
            "127.0.0.1",
            "::1",
            "172.17.0.1",
            "192.168.1.9",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(default.contains(ip(addr)), "{addr}");
        }
        assert!(!default.contains(ip("203.0.113.9")));
    }

    fn walk(peer: Option<&str>, lines: &[&str], trusted: &TrustedProxies) -> ClientAddress {
        let lines: Vec<&[u8]> = lines.iter().map(|l| l.as_bytes()).collect();
        client_address(peer.map(ip), &lines, trusted)
    }

    #[test]
    fn untrusted_peer_is_the_client_whatever_the_header_says() {
        let trusted = TrustedProxies::default_ranges();
        assert_eq!(
            walk(Some("203.0.113.9"), &["1.2.3.4"], &trusted),
            ClientAddress::Direct("203.0.113.9".into())
        );
        let none = TrustedProxies::none();
        assert_eq!(
            walk(Some("127.0.0.1"), &["1.2.3.4"], &none),
            ClientAddress::Direct("127.0.0.1".into())
        );
    }

    #[test]
    fn no_peer_ignores_the_header() {
        let trusted = TrustedProxies::default_ranges();
        assert_eq!(
            walk(None, &["198.51.100.7"], &trusted),
            ClientAddress::Unknown
        );
        assert_eq!(walk(None, &[], &trusted), ClientAddress::Unknown);
    }

    #[test]
    fn trusted_peer_header_is_walked_past_trusted_hops() {
        let trusted = TrustedProxies::default_ranges();
        let peer = Some("127.0.0.1");
        // The client forged the leftmost value; the proxy appended the
        // address it saw.
        assert_eq!(
            walk(peer, &["6.6.6.6, 198.51.100.7"], &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
        // Two proxies: a CDN edge on a private address in front.
        assert_eq!(
            walk(peer, &["6.6.6.6, 198.51.100.7, 10.0.0.2"], &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
        assert_eq!(
            walk(peer, &["192.168.1.20"], &trusted),
            ClientAddress::Forwarded("192.168.1.20".into())
        );
        assert_eq!(
            walk(peer, &[], &trusted),
            ClientAddress::Proxy("127.0.0.1".into())
        );
        assert!(!ClientAddress::Proxy("127.0.0.1".into()).is_individual());
        assert!(ClientAddress::Direct("203.0.113.9".into()).is_individual());
    }

    #[test]
    fn every_header_line_is_read_in_order() {
        // HAProxy (`option forwardfor`) adds its own line after the one the
        // client sent: the last line is the proxy's, not the first.
        let trusted = TrustedProxies::default_ranges();
        assert_eq!(
            walk(Some("127.0.0.1"), &["6.6.6.6", "198.51.100.7"], &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
    }

    #[test]
    fn ports_and_brackets_do_not_make_new_keys() {
        let trusted = TrustedProxies::default_ranges();
        let peer = Some("127.0.0.1");
        for hop in ["198.51.100.7:5678", "198.51.100.7:1"] {
            assert_eq!(
                walk(peer, &[hop], &trusted),
                ClientAddress::Forwarded("198.51.100.7".into()),
                "{hop}"
            );
        }
        for hop in ["[2001:db8::1]:443", "[2001:db8::1]", "2001:db8::1"] {
            assert_eq!(
                walk(peer, &[hop], &trusted),
                ClientAddress::Forwarded("2001:db8::1".into()),
                "{hop}"
            );
        }
    }

    #[test]
    fn garbage_stops_the_walk_in_a_bucket_of_its_own() {
        // Not the proxy's address: garbage must not fill the bucket every
        // client behind the proxy would then share.
        let trusted = TrustedProxies::default_ranges();
        let peer = Some(ip("127.0.0.1"));
        for line in [
            b"unknown".as_slice(),
            b"proxy.local",
            b"1.2.3.4:99999",
            b"[2001:db8::1]:x",
            b"\xff\xfe",
            b"6.6.6.6, \xc3",
        ] {
            let got = client_address(peer, &[line], &trusted);
            assert_eq!(got, ClientAddress::Unparseable, "{line:?}");
            assert_eq!(got.ip(), "unparseable");
            assert!(!got.is_individual());
        }
        // Garbage left of the client the proxy named is never reached.
        assert_eq!(
            walk(Some("127.0.0.1"), &["garbage, 198.51.100.7"], &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
    }

    #[test]
    fn a_range_trusting_everyone_is_refused() {
        for raw in ["0.0.0.0/0", "::/0", "127.0.0.1, ::/0"] {
            assert!(
                matches!(
                    TrustedProxies::parse(raw),
                    Err(ListError::TrustsEveryone(_))
                ),
                "{raw}"
            );
        }
        assert!(TrustedProxies::parse("0.0.0.0/1").is_ok());
    }

    #[test]
    fn rate_keys_group_ipv6_by_64() {
        assert_eq!(
            rate_key("2001:db8:1:2::1"),
            rate_key("2001:db8:1:2:ffff:abcd:1:2")
        );
        assert_ne!(rate_key("2001:db8:1:2::1"), rate_key("2001:db8:1:3::1"));
        assert_eq!(rate_key("::ffff:203.0.113.4"), "203.0.113.4");
        assert_eq!(rate_key("203.0.113.4"), "203.0.113.4");
        assert_eq!(rate_key("unknown"), "unknown");
    }
}
