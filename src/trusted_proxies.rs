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
    /// ranges, or `none`. An empty value is the default list. `Err` names
    /// the first entry that is neither.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(Self::default_ranges());
        }
        if raw.eq_ignore_ascii_case("none") {
            return Ok(Self::none());
        }
        raw.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|entry| !entry.is_empty())
            .map(|entry| Cidr::parse(entry).ok_or_else(|| entry.to_string()))
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

static FROM_ENV: OnceLock<Option<TrustedProxies>> = OnceLock::new();
static FROM_DB: RwLock<Option<TrustedProxies>> = RwLock::new(None);

/// `CALRS_TRUSTED_PROXIES`, read once. An unusable value trusts no peer
/// (and says so in the log): failing open would hand every client a forged
/// address.
fn from_env() -> Option<&'static TrustedProxies> {
    FROM_ENV
        .get_or_init(|| {
            let raw = std::env::var("CALRS_TRUSTED_PROXIES").ok()?;
            Some(TrustedProxies::parse(&raw).unwrap_or_else(|entry| {
                tracing::error!(
                    entry = %entry,
                    "CALRS_TRUSTED_PROXIES: not an address or CIDR range; trusting no proxy"
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
        TrustedProxies::parse(r).unwrap_or_else(|entry| {
            tracing::error!(entry = %entry, "saved trusted proxies: not an address or CIDR range; trusting no proxy");
            TrustedProxies::none()
        })
    });
    if let Ok(mut g) = FROM_DB.write() {
        *g = list;
    }
}

/// The client address a request names, given the TCP peer (`None` when the
/// router runs without a socket, as in tests, where the header is taken as
/// coming from a trusted proxy) and the `X-Forwarded-For` value.
///
/// An untrusted peer is the client, whatever the header says. A trusted
/// peer's header is read right to left past every trusted hop; the first
/// address that is not trusted is the client (all trusted: the leftmost).
/// A trusted peer with no header is itself the address, shared by every
/// client behind it.
pub fn client_address(
    peer: Option<IpAddr>,
    forwarded_for: Option<&str>,
    trusted: &TrustedProxies,
) -> ClientAddress {
    let peer = peer.map(|p| p.to_canonical());
    if let Some(p) = peer {
        if !trusted.contains(p) {
            return ClientAddress::Direct(p.to_string());
        }
    }
    let hops: Vec<&str> = forwarded_for
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|hop| !hop.is_empty())
        .collect();
    let mut client = None;
    for hop in hops.iter().rev() {
        match hop.parse::<IpAddr>() {
            Ok(ip) if trusted.contains(ip) => client = Some(ip.to_canonical().to_string()),
            Ok(ip) => return ClientAddress::Forwarded(ip.to_canonical().to_string()),
            Err(_) => return ClientAddress::Forwarded(hop.to_string()),
        }
    }
    match (client, peer) {
        (Some(leftmost), _) => ClientAddress::Forwarded(leftmost),
        (None, Some(p)) => ClientAddress::Proxy(p.to_string()),
        (None, None) => ClientAddress::Unknown,
    }
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
    /// No socket and no header.
    Unknown,
}

impl ClientAddress {
    /// The address, `"unknown"` when there is none.
    pub fn ip(&self) -> &str {
        match self {
            ClientAddress::Direct(ip) | ClientAddress::Forwarded(ip) | ClientAddress::Proxy(ip) => {
                ip
            }
            ClientAddress::Unknown => "unknown",
        }
    }

    /// Whether the address is one client's rather than one shared by every
    /// client behind a proxy (or unknown).
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
            Err("nope".to_string())
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

    #[test]
    fn untrusted_peer_is_the_client_whatever_the_header_says() {
        let trusted = TrustedProxies::default_ranges();
        assert_eq!(
            client_address(Some(ip("203.0.113.9")), Some("1.2.3.4"), &trusted),
            ClientAddress::Direct("203.0.113.9".into())
        );
        let none = TrustedProxies::none();
        assert_eq!(
            client_address(Some(ip("127.0.0.1")), Some("1.2.3.4"), &none),
            ClientAddress::Direct("127.0.0.1".into())
        );
    }

    #[test]
    fn trusted_peer_header_is_walked_past_trusted_hops() {
        let trusted = TrustedProxies::default_ranges();
        let peer = Some(ip("127.0.0.1"));
        // The client forged the leftmost value; the proxy appended the
        // address it saw.
        assert_eq!(
            client_address(peer, Some("6.6.6.6, 198.51.100.7"), &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
        // Two proxies: a CDN edge on a private address in front.
        assert_eq!(
            client_address(peer, Some("6.6.6.6, 198.51.100.7, 10.0.0.2"), &trusted),
            ClientAddress::Forwarded("198.51.100.7".into())
        );
        assert_eq!(
            client_address(peer, Some("192.168.1.20"), &trusted),
            ClientAddress::Forwarded("192.168.1.20".into())
        );
        assert_eq!(
            client_address(peer, None, &trusted),
            ClientAddress::Proxy("127.0.0.1".into())
        );
        assert_eq!(client_address(None, None, &trusted), ClientAddress::Unknown);
        assert!(!ClientAddress::Proxy("127.0.0.1".into()).is_individual());
        assert!(ClientAddress::Direct("203.0.113.9".into()).is_individual());
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
