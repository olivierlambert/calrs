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
//! targets.
//!
//! Proxies: calendar clients honour `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`
//! and `NO_PROXY` ([`ProxyConfig`]), since on-prem deployments often reach
//! the outside only through one. A proxied request cannot be pinned (the
//! proxy resolves the name), so for those the private-address filtering is
//! the proxy's job: calrs only resolves the proxy's own host, which is
//! exempt from the check, while the saved URL and the literal checks above
//! still apply. Hosts that `NO_PROXY` sends direct get the full pinning.
//! Booking webhooks use no proxy at all.
//!
//! Fixed provider endpoints (Google, Microsoft login and Graph) and the
//! admin-configured services outside the private-host policy (SMS gateway,
//! captcha, meeting webhook, OIDC issuer) skip the guarded resolver: their
//! hosts are constants or the admin's choice. They still take proxies the
//! way the guarded clients do, through [`plain_builder`].

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
    /// Addresses compare as addresses, so `fd00:0::1` matches `fd00::1`;
    /// names compare without regard to case.
    pub fn allows(&self, host: &str) -> bool {
        let host = normalize_host(host);
        match self {
            Allowlist::Settings => {
                #[cfg(test)]
                if test_loopback_allowed(&host) {
                    return true;
                }
                crate::settings::private_host_allowlist()
                    .iter()
                    .any(|h| normalize_host(h) == host)
            }
            Allowlist::Fixed(list) => list.contains(&host),
        }
    }
}

/// The form a host is compared in: an IP address in its canonical spelling
/// (brackets dropped), a name lowercased.
fn normalize_host(host: &str) -> String {
    let bare = host.trim().trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(ip) => ip.to_string(),
        Err(_) => bare.to_ascii_lowercase(),
    }
}

// Tests only. The test suite runs CalDAV, EWS and feed clients against
// loopback servers on `127.0.0.1`; every one of them would otherwise need the
// process-wide allowlist (and `ENV_LOCK`). So under `cfg(test)` the
// configured allowlist also lets loopback literals through, on every thread
// that has not called [`strict`]. Names are never let through this way.
#[cfg(test)]
thread_local! {
    static STRICT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn test_loopback_allowed(host: &str) -> bool {
    !STRICT.with(|s| s.get()) && host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Tests only: until the guard drops, this thread applies the configured
/// allowlist exactly as production does.
#[cfg(test)]
pub(crate) fn strict() -> StrictGuard {
    STRICT.with(|s| s.set(true));
    StrictGuard
}

#[cfg(test)]
pub(crate) struct StrictGuard;

#[cfg(test)]
impl Drop for StrictGuard {
    fn drop(&mut self) {
        STRICT.with(|s| s.set(false));
    }
}

/// The error text of a refused request. It names no address: it reaches the
/// sources page and the owner's alert email, and an internal address is the
/// admin's business (it is logged at warn level).
pub const REFUSED: &str =
    "the address of this server is private or reserved, which is not allowed (ask your administrator)";

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
            // No address: see [`REFUSED`].
            GuardError::Private(_) => f.write_str(REFUSED),
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
    /// The configured proxies' host names: the operator chose them, and they
    /// usually sit on a private address.
    proxy_hosts: Arc<[String]>,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let lookup = Arc::clone(&self.lookup);
        let host = name.as_str().to_string();
        let allowlist = if self.proxy_hosts.contains(&normalize_host(&host)) {
            Allowlist::fixed(std::slice::from_ref(&host))
        } else {
            self.allowlist.clone()
        };
        Box::pin(async move {
            match resolve_checked(lookup.as_ref(), &host, &allowlist).await {
                // Port 0: the connector then uses the URL's port (or the
                // scheme's default). reqwest 0.12 would otherwise keep a
                // non-zero port from the resolver when the URL names none,
                // where 0.11 always used the URL's.
                Ok(addrs) => Ok(Box::new(addrs.into_iter().map(|mut a| {
                    a.set_port(0);
                    a
                })) as reqwest::dns::Addrs),
                Err(e) => {
                    if let GuardError::Private(ip) = &e {
                        tracing::warn!(host = %host, %ip, "outbound connection refused by the private-host policy");
                    }
                    Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                }
            }
        })
    }
}

/// Proxies from the standard environment variables, in calrs's own order:
/// `HTTP_PROXY`, then `http_proxy` (only `http_proxy` under CGI, where a
/// request header can set `HTTP_PROXY`); `HTTPS_PROXY`, then `https_proxy`;
/// `ALL_PROXY`, then `all_proxy`, for a scheme with neither; `NO_PROXY`,
/// then `no_proxy`. Only `http://` and `https://` proxies are kept (see
/// [`usable_proxy`]): this build has no SOCKS support, so a `socks5://`
/// proxy is dropped (see `ignored`) and its requests go direct, checked and
/// pinned like any other.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Option<String>,
    /// A proxy variable was set but reqwest refused its value.
    pub ignored: bool,
}

impl ProxyConfig {
    /// No proxy at all (booking webhooks).
    pub fn none() -> Self {
        Self::default()
    }

    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// [`ProxyConfig::from_env`] over any variable source.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let get = |names: &[&str]| {
            names
                .iter()
                .filter_map(|n| var(n))
                .map(|v| v.trim().to_string())
                .find(|v| !v.is_empty())
        };
        let all = get(&["ALL_PROXY", "all_proxy"]);
        let http = if var("REQUEST_METHOD").is_some() {
            get(&["http_proxy"])
        } else {
            get(&["HTTP_PROXY", "http_proxy"])
        };
        let http = http.or_else(|| all.clone());
        let https = get(&["HTTPS_PROXY", "https_proxy"]).or(all);
        let ignored = http.as_deref().is_some_and(|p| !usable_proxy(p))
            || https.as_deref().is_some_and(|p| !usable_proxy(p));
        Self {
            http: http.filter(|p| usable_proxy(p)),
            https: https.filter(|p| usable_proxy(p)),
            no_proxy: get(&["NO_PROXY", "no_proxy"]),
            ignored,
        }
    }

    pub fn is_set(&self) -> bool {
        self.http.is_some() || self.https.is_some()
    }

    /// The host names of the proxies in use, lowercased, for the startup
    /// log, the resolver's exemption and [`url_refused`]. Never the URLs:
    /// they can carry credentials.
    pub fn hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = [&self.http, &self.https]
            .into_iter()
            .flatten()
            .filter_map(|p| proxy_url(p)?.host_str().map(normalize_host))
            .collect();
        hosts.dedup();
        hosts
    }

    fn apply(&self, mut builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let no_proxy = || reqwest::NoProxy::from_string(self.no_proxy.as_deref().unwrap_or(""));
        if let Some(Ok(proxy)) = self.http.as_deref().map(reqwest::Proxy::http) {
            builder = builder.proxy(proxy.no_proxy(no_proxy()));
        }
        if let Some(Ok(proxy)) = self.https.as_deref().map(reqwest::Proxy::https) {
            builder = builder.proxy(proxy.no_proxy(no_proxy()));
        }
        builder
    }
}

/// A proxy setting as reqwest reads it: a URL with a host, or, when it does
/// not parse as one (`10.0.0.8:3128`, `localhost:3128`), `http://` in front
/// of it.
fn proxy_url(raw: &str) -> Option<Url> {
    Url::parse(raw)
        .ok()
        .filter(|u| u.has_host())
        .or_else(|| Url::parse(&format!("http://{raw}")).ok())
}

/// Whether calrs can use this proxy setting: reqwest accepts it and it is
/// an HTTP or HTTPS proxy. reqwest 0.11 refused every other scheme itself;
/// 0.12 accepts any URL here and, built without its `socks` feature, would
/// not speak SOCKS to it, so the scheme is checked by calrs.
fn usable_proxy(raw: &str) -> bool {
    reqwest::Proxy::http(raw).is_ok()
        && proxy_url(raw).is_some_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// Log, once per process, how calendar requests leave: through a proxy
/// (private-address filtering is then the proxy's job), or direct because
/// the proxy setting was refused.
pub fn log_proxy_posture() {
    let proxies = ProxyConfig::from_env();
    if proxies.is_set() {
        tracing::warn!(
            proxy_hosts = ?proxies.hosts(),
            no_proxy = proxies.no_proxy.as_deref().unwrap_or(""),
            "calendar requests go through a proxy: the proxy resolves their host names, so \
             filtering private addresses for them is the proxy's job (hosts in NO_PROXY are \
             still checked and pinned by calrs)"
        );
    }
    if proxies.ignored {
        tracing::warn!(
            "a proxy setting was ignored (unsupported, for instance SOCKS): the requests it \
             would have carried go direct, checked and pinned by calrs"
        );
    }
}

/// Whether a user- or server-supplied URL must not be requested at all: it
/// names a private IP literal, or the host of a proxy in use, that is not on
/// the allowlist. Literals never reach the guarded resolver, and the
/// resolver lets a proxy's host through for the proxy connection, so these
/// are refused before any request. Such URLs never need to reach the proxy
/// itself. Logs the reason.
pub fn url_refused(url: &Url) -> bool {
    url_refused_with(url, &Allowlist::Settings, &ProxyConfig::from_env().hosts())
}

/// [`url_refused`] with a given allowlist and proxy hosts.
pub fn url_refused_with(url: &Url, allowlist: &Allowlist, proxy_hosts: &[String]) -> bool {
    if let Some(ip) = literal_refused(url, allowlist) {
        tracing::warn!(%ip, "request to a private or reserved address refused");
        return true;
    }
    if let Some(host) = url.host_str().map(normalize_host) {
        if proxy_hosts.contains(&host) && !allowlist.allows(&host) {
            tracing::warn!(host = %host, "request to the proxy's own host refused");
            return true;
        }
    }
    false
}

/// A client builder for a user- or admin-supplied URL: guarded resolution
/// against the configured allowlist, the environment's proxies, no
/// redirects (set [`redirects`] to follow some).
pub fn client_builder() -> reqwest::ClientBuilder {
    builder_with(
        Allowlist::Settings,
        Arc::new(SystemLookup),
        &ProxyConfig::from_env(),
    )
}

/// [`client_builder`] with a given allowlist, resolver and proxies.
pub fn builder_with(
    allowlist: Allowlist,
    lookup: Arc<dyn Lookup>,
    proxies: &ProxyConfig,
) -> reqwest::ClientBuilder {
    // `no_proxy()` drops reqwest's own reading of the environment; the
    // proxies wanted are added back explicitly.
    let builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(GuardedResolver {
            lookup,
            allowlist,
            proxy_hosts: proxies.hosts().into(),
        }));
    proxies.apply(builder)
}

/// A client builder for a fixed endpoint or an admin-configured service
/// outside the private-host policy: no guarded resolver, no redirects
/// policy of its own (set one), and the environment's proxies exactly as
/// [`client_builder`] takes them (only `http://` and `https://` proxies,
/// `NO_PROXY` honoured). reqwest 0.12's own reading of the environment
/// would take a `socks5://` proxy and, built without SOCKS, speak plain
/// HTTP to it; 0.11 dropped it and went direct, as this does.
pub fn plain_builder() -> reqwest::ClientBuilder {
    plain_builder_with(&ProxyConfig::from_env())
}

/// [`plain_builder`] with given proxies.
pub fn plain_builder_with(proxies: &ProxyConfig) -> reqwest::ClientBuilder {
    proxies.apply(reqwest::Client::builder().no_proxy())
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

/// Whether a redirect to `target` is refused: see [`url_refused_with`].
fn redirect_refused(target: &Url, allowlist: &Allowlist, proxy_hosts: &[String]) -> bool {
    url_refused_with(target, allowlist, proxy_hosts)
}

/// Follow redirects while fewer than `max` URLs have been requested in the
/// chain, so at most `max - 1` of them, refusing those [`redirect_refused`]
/// names. Host names in a redirect go through the guarded resolver.
///
/// `previous()` holds every URL already requested, the original included,
/// in reqwest 0.11 and 0.12 alike. This is the count `Policy::limited(max)`
/// kept in 0.11, which calrs's CalDAV client used before #363; 0.12's
/// `limited(max)` follows one more, so this policy keeps its own count
/// rather than delegating.
pub fn redirects(max: usize, allowlist: Allowlist) -> reqwest::redirect::Policy {
    redirects_with(max, allowlist, ProxyConfig::from_env().hosts())
}

/// [`redirects`] with given proxy hosts.
pub fn redirects_with(
    max: usize,
    allowlist: Allowlist,
    proxy_hosts: Vec<String>,
) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max {
            return attempt.error("too many redirects");
        }
        if redirect_refused(attempt.url(), &allowlist, &proxy_hosts) {
            return attempt.error("redirect to a private or reserved address refused");
        }
        attempt.follow()
    })
}

#[cfg(test)]
mod tests;
