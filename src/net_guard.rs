//! Outbound HTTP to URLs a user or an admin supplied (#363).
//!
//! CalDAV and EWS sources, resource feeds and booking webhooks are checked
//! against the private-host policy ([`crate::caldav::is_private_ip`] plus the
//! private-host allowlist of [`crate::settings`]) when they are saved. A check
//! that looks the host up and then lets the HTTP client look it up again is
//! not enough: the second answer can differ from the first. Every client built
//! by [`client_builder`] resolves through [`GuardedResolver`] instead, which
//! looks the name up once per connection, refuses it when any address is
//! private or reserved (unless the host is allowlisted), and hands the
//! connector only the addresses it checked.
//!
//! Hosts written as IP literals never reach a resolver. The saved URL's own
//! literal is checked when it is saved; [`literal_refused`] checks the URLs a
//! server hands back (CalDAV hrefs), and [`redirects`] checks redirect
//! targets. Proxies are ignored, since a proxy resolves the host itself.
//!
//! Fixed provider endpoints (Google, Microsoft login and Graph) do not use
//! this: their hosts are constants, not configuration.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;

/// How long one name lookup may take.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// What a [`Lookup`] returns.
pub type LookupFuture = Pin<Box<dyn Future<Output = std::io::Result<Vec<SocketAddr>>> + Send>>;

/// Name resolution, injectable so tests can answer differently from one
/// lookup to the next.
pub trait Lookup: Send + Sync {
    fn lookup(&self, host: &str) -> LookupFuture;
}

/// The system resolver (`getaddrinfo` on the blocking pool).
pub struct SystemLookup;

impl Lookup for SystemLookup {
    fn lookup(&self, host: &str) -> LookupFuture {
        let host = host.to_string();
        Box::pin(async move {
            let found =
                tokio::time::timeout(LOOKUP_TIMEOUT, tokio::net::lookup_host((host.as_str(), 0)))
                    .await
                    .map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::TimedOut, "lookup timed out")
                    })??;
            Ok(found.collect())
        })
    }
}

/// Hosts allowed to resolve to private or reserved addresses.
#[derive(Clone, Debug)]
pub enum Allowlist {
    /// The configured private-host allowlist, read at every check so a change
    /// applies to clients that already exist.
    Settings,
    /// A list given by the caller.
    Fixed(Arc<[String]>),
}

impl Allowlist {
    pub fn fixed(hosts: &[String]) -> Self {
        Allowlist::Fixed(hosts.iter().map(|h| normalize_host(h)).collect())
    }

    /// Whether `host` (a name or an IP, brackets optional) is on the list.
    pub fn allows(&self, host: &str) -> bool {
        let host = normalize_host(host);
        match self {
            Allowlist::Settings => crate::settings::private_host_allowlist()
                .iter()
                .any(|h| normalize_host(h) == host),
            Allowlist::Fixed(list) => list.contains(&host),
        }
    }
}

fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

/// Why a host was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    /// It resolves to (or is) a private or reserved address.
    Private(IpAddr),
    /// It does not resolve.
    Unresolvable(String),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Private(ip) => {
                write!(f, "resolves to a private or reserved address ({ip})")
            }
            GuardError::Unresolvable(e) => write!(f, "cannot resolve the host: {e}"),
        }
    }
}

impl std::error::Error for GuardError {}

/// Look `host` up once and return its addresses, refused when any of them is
/// private or reserved and the host is not allowlisted. The caller connects
/// to exactly these addresses.
pub async fn resolve_checked(
    lookup: &dyn Lookup,
    host: &str,
    allowlist: &Allowlist,
) -> Result<Vec<SocketAddr>, GuardError> {
    let addrs = lookup
        .lookup(host)
        .await
        .map_err(|e| GuardError::Unresolvable(e.to_string()))?;
    if addrs.is_empty() {
        return Err(GuardError::Unresolvable("no address".to_string()));
    }
    if !allowlist.allows(host) {
        if let Some(bad) = addrs.iter().find(|a| crate::caldav::is_private_ip(&a.ip())) {
            return Err(GuardError::Private(bad.ip()));
        }
    }
    Ok(addrs)
}

/// The reqwest resolver behind [`client_builder`]: [`resolve_checked`] for
/// every connection the client opens, redirects and other hosts included.
pub struct GuardedResolver {
    lookup: Arc<dyn Lookup>,
    allowlist: Allowlist,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: hyper::client::connect::dns::Name) -> reqwest::dns::Resolving {
        let lookup = Arc::clone(&self.lookup);
        let allowlist = self.allowlist.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            match resolve_checked(lookup.as_ref(), &host, &allowlist).await {
                Ok(addrs) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Err(e) => {
                    if matches!(e, GuardError::Private(_)) {
                        tracing::warn!(host = %host, error = %e, "outbound connection refused by the private-host policy");
                    }
                    Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                }
            }
        })
    }
}

/// A client builder for a user- or admin-supplied URL: guarded resolution
/// against the configured allowlist, no proxy, no redirects (set
/// [`redirects`] to follow some).
pub fn client_builder() -> reqwest::ClientBuilder {
    builder_with(Allowlist::Settings, Arc::new(SystemLookup))
}

/// [`client_builder`] with a given allowlist and resolver.
pub fn builder_with(allowlist: Allowlist, lookup: Arc<dyn Lookup>) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(GuardedResolver { lookup, allowlist }))
}

/// The IP address a URL names directly, `None` for a host name.
/// The URL parser has already put any IPv4 form (`2130706433`, `0x7f.1`)
/// into dotted notation, so parsing the host string is enough.
pub fn literal_ip(url: &Url) -> Option<IpAddr> {
    url.host_str()?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()
}

/// The private or reserved IP literal `url` names, `None` when it names a
/// host name (the resolver checks those), a public address or an
/// allowlisted one.
pub fn literal_refused(url: &Url, allowlist: &Allowlist) -> Option<IpAddr> {
    let ip = literal_ip(url)?;
    if !crate::caldav::is_private_ip(&ip) || allowlist.allows(&ip.to_string()) {
        return None;
    }
    Some(ip)
}

/// Whether a redirect to `target` is refused: a private IP literal that is
/// neither allowlisted nor the host the request started at (checked when it
/// was saved).
fn redirect_refused(target: &Url, first: Option<&Url>, allowlist: &Allowlist) -> bool {
    let start = first.and_then(Url::host_str);
    if start.is_some() && start == target.host_str() {
        return false;
    }
    literal_refused(target, allowlist).is_some()
}

/// Follow at most `max` redirects, refusing those [`redirect_refused`]
/// names. Host names in a redirect go through the guarded resolver.
pub fn redirects(max: usize, allowlist: Allowlist) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() > max {
            return attempt.error("too many redirects");
        }
        if redirect_refused(attempt.url(), attempt.previous().first(), &allowlist) {
            return attempt.error("redirect to a private or reserved address refused");
        }
        attempt.follow()
    })
}

#[cfg(test)]
mod tests;
