use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;

/// Answers `first` to the first lookup and `then` to every later one, the
/// way a rebinding name server would.
struct Flipping {
    calls: AtomicUsize,
    first: SocketAddr,
    then: SocketAddr,
}

impl Flipping {
    fn new(first: &str, then: SocketAddr) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            first: SocketAddr::new(first.parse().unwrap(), 0),
            then,
        })
    }
}

impl Lookup for Flipping {
    fn lookup(&self, _host: &str) -> LookupFuture {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let addr = if n == 0 { self.first } else { self.then };
        Box::pin(async move { Ok(vec![addr]) })
    }
}

/// Always answers the same addresses.
struct Fixed(Vec<SocketAddr>);

impl Lookup for Fixed {
    fn lookup(&self, _host: &str) -> LookupFuture {
        let addrs = self.0.clone();
        Box::pin(async move { Ok(addrs) })
    }
}

fn addr(ip: &str) -> SocketAddr {
    SocketAddr::new(ip.parse().unwrap(), 0)
}

/// A loopback server answering 200 to anything, counting connections.
async fn counting_server() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&hits);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });
    (local, hits)
}

#[tokio::test]
async fn a_name_that_rebinds_to_a_private_address_is_never_connected_to() {
    let (server, hits) = counting_server().await;
    let lookup = Flipping::new("93.184.215.14", server);
    let none = Allowlist::fixed(&[]);

    // The check at save time sees the public answer.
    let checked = resolve_checked(lookup.as_ref(), "rebind.test", &none).await;
    assert_eq!(checked.unwrap(), vec![addr("93.184.215.14")]);

    // The request resolves again, gets the loopback answer, and is refused
    // before any connection is opened.
    let client = builder_with(none, lookup.clone(), &ProxyConfig::none())
        .build()
        .unwrap();
    let url = format!("http://rebind.test:{}/", server.port());
    let err = client.get(&url).send().await.unwrap_err();
    assert!(err.is_connect(), "{err:?}");
    assert_eq!(lookup.calls.load(Ordering::SeqCst), 2);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_allowlisted_name_may_resolve_to_a_private_address() {
    let (server, hits) = counting_server().await;
    let lookup = Arc::new(Fixed(vec![server]));
    let list = Allowlist::fixed(&["Radicale".to_string()]);
    let client = builder_with(list, lookup, &ProxyConfig::none())
        .build()
        .unwrap();
    let url = format!("http://radicale:{}/", server.port());
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_configured_allowlist_is_read_at_connection_time() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = crate::test_support::EnvGuard::new();
    let (server, hits) = counting_server().await;
    let lookup = Arc::new(Fixed(vec![server]));
    let client = builder_with(Allowlist::Settings, lookup, &ProxyConfig::none())
        .build()
        .unwrap();
    let url = format!("http://radicale:{}/", server.port());

    assert!(client.get(&url).send().await.is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    // Same client, allowlist set afterwards.
    std::env::set_var("CALRS_ALLOW_PRIVATE_HOSTS", "other, radicale");
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn one_private_address_among_public_ones_refuses_the_name() {
    let lookup = Fixed(vec![addr("93.184.215.14"), addr("fd00::1")]);
    let none = Allowlist::fixed(&[]);
    assert_eq!(
        resolve_checked(&lookup, "mixed.test", &none).await,
        Err(GuardError::Private("fd00::1".parse().unwrap()))
    );
    // IPv6 forms that carry a private IPv4 address count as private.
    for ip in [
        "::ffff:10.0.0.1",
        "64:ff9b::7f00:1",
        "2002:c0a8:101::1",
        "fe80::1",
    ] {
        let lookup = Fixed(vec![addr(ip)]);
        assert!(
            matches!(
                resolve_checked(&lookup, "v6.test", &none).await,
                Err(GuardError::Private(_))
            ),
            "{ip}"
        );
    }
    let empty = Fixed(vec![]);
    assert!(matches!(
        resolve_checked(&empty, "nothing.test", &none).await,
        Err(GuardError::Unresolvable(_))
    ));
}

#[test]
fn literals_are_refused_unless_public_or_allowlisted() {
    let none = Allowlist::fixed(&[]);
    let url = |s: &str| Url::parse(s).unwrap();
    assert_eq!(
        literal_refused(&url("http://10.1.2.3/cal/"), &none),
        Some("10.1.2.3".parse().unwrap())
    );
    // The parser normalises other IPv4 spellings first.
    assert!(literal_refused(&url("http://2130706433/"), &none).is_some());
    assert!(literal_refused(&url("http://[::1]:8080/"), &none).is_some());
    assert!(literal_refused(&url("https://93.184.215.14/"), &none).is_none());
    assert!(literal_refused(&url("https://cal.example.com/"), &none).is_none());

    let list = Allowlist::fixed(&["127.0.0.1".to_string(), "[::1]".to_string()]);
    assert!(literal_refused(&url("http://127.0.0.1:5232/"), &list).is_none());
    assert!(literal_refused(&url("http://[::1]/"), &list).is_none());
    assert!(literal_refused(&url("http://127.0.0.2/"), &list).is_some());
}

#[test]
fn redirects_to_a_private_literal_are_refused() {
    let _strict = strict();
    let none = Allowlist::fixed(&[]);
    let url = |s: &str| Url::parse(s).unwrap();
    assert!(redirect_refused(
        &url("http://169.254.169.254/latest/"),
        &none,
        &[]
    ));
    // Names are left to the resolver.
    assert!(!redirect_refused(
        &url("https://other.example.com/"),
        &none,
        &[]
    ));
    // No exemption for the host the request started at: a saved literal is
    // only reachable while it is allowlisted.
    assert!(redirect_refused(
        &url("http://127.0.0.1:5232/dav/"),
        &none,
        &[]
    ));
    let list = Allowlist::fixed(&["10.0.0.5".to_string()]);
    assert!(!redirect_refused(&url("http://10.0.0.5/"), &list, &[]));
}

#[test]
fn allowlist_entries_compare_as_addresses() {
    let list = Allowlist::fixed(&[
        "fd00:0::1".to_string(),
        "[FE80::0:1]".to_string(),
        "Radicale".to_string(),
    ]);
    assert!(list.allows("fd00::1"));
    assert!(list.allows("[fd00:0:0::1]"));
    assert!(list.allows("fe80::1"));
    assert!(list.allows("RADICALE"));
    assert!(!list.allows("fd00::2"));
    let url = Url::parse("http://[fd00::1]:8080/").unwrap();
    assert!(literal_refused(&url, &list).is_none());
}

#[tokio::test]
async fn a_refusal_names_no_address() {
    let (server, _) = counting_server().await;
    let lookup = Flipping::new("10.9.8.7", server);
    let none = Allowlist::fixed(&[]);
    let client = builder_with(none, lookup, &ProxyConfig::none())
        .build()
        .unwrap();
    let err = client
        .get(format!("http://hidden.test:{}/", server.port()))
        .send()
        .await
        .unwrap_err();
    // The whole chain, as the sync stores it in `last_sync_error`.
    let shown = format!("{:#}", anyhow::Error::from(err));
    assert!(shown.contains(REFUSED), "{shown}");
    assert!(!shown.contains("10.9.8.7"), "{shown}");
    assert_eq!(
        GuardError::Private("10.9.8.7".parse().unwrap()).to_string(),
        REFUSED
    );
}

#[tokio::test]
async fn a_redirect_to_a_private_literal_is_not_followed() {
    // A server on a public-looking name (pinned to loopback through an
    // allowlisted lookup) redirecting to a private literal.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 302 Found\r\nlocation: http://10.255.255.1/x\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .await;
            });
        }
    });
    let list = Allowlist::fixed(&["calendar.test".to_string()]);
    let client = builder_with(
        list.clone(),
        Arc::new(Fixed(vec![local])),
        &ProxyConfig::none(),
    )
    .redirect(redirects(5, list))
    .build()
    .unwrap();
    let err = client
        .get(format!("http://calendar.test:{}/", local.port()))
        .send()
        .await
        .unwrap_err();
    assert!(err.is_redirect(), "{err:?}");
}

/// Answers one address for every name and records the names asked for.
struct Recording {
    addr: SocketAddr,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Lookup for Recording {
    fn lookup(&self, host: &str) -> LookupFuture {
        self.asked.lock().unwrap().push(host.to_string());
        let addr = self.addr;
        Box::pin(async move { Ok(vec![addr]) })
    }
}

#[test]
fn proxy_settings_are_read_like_reqwest_reads_them() {
    let vars = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
        }
    };
    assert!(!ProxyConfig::from_vars(vars(&[])).is_set());
    let cfg = ProxyConfig::from_vars(vars(&[
        ("HTTP_PROXY", "http://upper:3128"),
        ("http_proxy", "http://lower:3128"),
        ("ALL_PROXY", "http://all:3128"),
        ("no_proxy", "intranet.example, 10.0.0.0/8"),
    ]));
    assert_eq!(cfg.http.as_deref(), Some("http://upper:3128"));
    assert_eq!(cfg.https.as_deref(), Some("http://all:3128"));
    assert_eq!(
        cfg.no_proxy.as_deref(),
        Some("intranet.example, 10.0.0.0/8")
    );
    assert_eq!(cfg.hosts(), vec!["upper", "all"]);
    // Under CGI a request header can set HTTP_PROXY.
    let cgi = ProxyConfig::from_vars(vars(&[
        ("REQUEST_METHOD", "GET"),
        ("HTTP_PROXY", "http://evil:3128"),
    ]));
    assert!(!cgi.is_set());
    // Credentials never reach the host list; a bare IP and port is read as
    // http, as reqwest reads it.
    let creds = ProxyConfig::from_vars(vars(&[("https_proxy", "http://user:pw@Squid.corp:3128")]));
    assert_eq!(creds.hosts(), vec!["squid.corp"]);
    let bare = ProxyConfig::from_vars(vars(&[("HTTP_PROXY", "10.0.0.8:3128")]));
    assert_eq!(bare.hosts(), vec!["10.0.0.8"]);
}

#[test]
fn a_socks_proxy_this_build_cannot_use_is_dropped_with_its_exemption() {
    let vars = |name: &str| (name == "ALL_PROXY").then(|| "socks5h://localhost:1080".to_string());
    let cfg = ProxyConfig::from_vars(vars);
    assert!(!cfg.is_set());
    assert!(cfg.ignored);
    assert!(cfg.hosts().is_empty());
}

#[tokio::test]
async fn with_a_dropped_socks_proxy_localhost_is_not_exempt() {
    let (server, hits) = counting_server().await;
    let cfg = ProxyConfig::from_vars(|name: &str| {
        (name == "ALL_PROXY").then(|| "socks5h://localhost:1080".to_string())
    });
    let client = builder_with(Allowlist::fixed(&[]), Arc::new(Fixed(vec![server])), &cfg)
        .build()
        .unwrap();
    let url = format!("http://localhost:{}/", server.port());
    assert!(client.get(&url).send().await.unwrap_err().is_connect());
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[test]
fn urls_naming_the_proxy_host_are_refused_unless_allowlisted() {
    let _strict = strict();
    let url = |s: &str| Url::parse(s).unwrap();
    let none = Allowlist::fixed(&[]);
    let proxy = vec!["localhost".to_string()];
    // A server href or redirect to the proxy's host, on any port.
    assert!(url_refused_with(
        &url("http://localhost:6379/"),
        &none,
        &proxy
    ));
    assert!(url_refused_with(
        &url("https://LOCALHOST/dav/"),
        &none,
        &proxy
    ));
    assert!(!url_refused_with(
        &url("https://cal.example.com/"),
        &none,
        &proxy
    ));
    // Literals stay refused whatever the proxies.
    assert!(url_refused_with(&url("http://10.1.2.3/"), &none, &[]));
    let list = Allowlist::fixed(&["localhost".to_string()]);
    assert!(!url_refused_with(
        &url("http://localhost:6379/"),
        &list,
        &proxy
    ));
}

/// A server on `front.test` that redirects to `location`.
async fn redirecting_server(location: String) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let location = location.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let reply = format!(
                    "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
                let _ = sock.write_all(reply.as_bytes()).await;
            });
        }
    });
    local
}

/// The usual corporate setup: a local proxy (cntlm, px) at
/// `http://localhost:3128` with `NO_PROXY=localhost`. A redirect to
/// `localhost` on another port goes direct, where the resolver would let
/// the proxy's name through: the redirect policy refuses it first.
#[tokio::test]
async fn a_redirect_to_the_proxy_host_on_another_port_is_refused() {
    let (internal, internal_hits) = counting_server().await;
    let front = redirecting_server(format!("http://localhost:{}/", internal.port())).await;
    let proxies = ProxyConfig {
        http: Some("http://localhost:3128".to_string()),
        no_proxy: Some("localhost, front.test".to_string()),
        ..ProxyConfig::none()
    };
    let list = Allowlist::fixed(&["front.test".to_string()]);
    let lookup = Arc::new(Fixed(vec![SocketAddr::new(
        "127.0.0.1".parse().unwrap(),
        0,
    )]));
    let client = builder_with(list.clone(), lookup, &proxies)
        .redirect(redirects_with(10, list, proxies.hosts()))
        .build()
        .unwrap();
    let err = client
        .get(format!("http://front.test:{}/", front.port()))
        .send()
        .await
        .unwrap_err();
    assert!(err.is_redirect(), "{err:?}");
    assert_eq!(internal_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_proxied_request_goes_to_the_proxy_whose_host_is_exempt() {
    let (proxy, hits) = counting_server().await;
    let lookup = Arc::new(Recording {
        addr: proxy,
        asked: Default::default(),
    });
    let proxies = ProxyConfig {
        http: Some(format!("http://proxy.corp:{}", proxy.port())),
        ..ProxyConfig::none()
    };
    let client = builder_with(Allowlist::fixed(&[]), lookup.clone(), &proxies)
        .build()
        .unwrap();
    let resp = client
        .get("http://calendar.example.com/dav/")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Only the proxy's own name was resolved here (to a loopback address,
    // allowed because it is the configured proxy); the target is the
    // proxy's to resolve.
    assert_eq!(*lookup.asked.lock().unwrap(), vec!["proxy.corp"]);
}

#[tokio::test]
async fn a_no_proxy_host_is_resolved_and_pinned_by_calrs() {
    let (proxy, proxy_hits) = counting_server().await;
    let (target, target_hits) = counting_server().await;
    let proxies = ProxyConfig {
        http: Some(format!("http://{proxy}")),
        no_proxy: Some("rebind.test".to_string()),
        ..ProxyConfig::none()
    };
    let lookup = Flipping::new("93.184.215.14", target);
    let none = Allowlist::fixed(&[]);
    assert!(resolve_checked(lookup.as_ref(), "rebind.test", &none)
        .await
        .is_ok());
    let client = builder_with(none, lookup.clone(), &proxies)
        .build()
        .unwrap();
    let url = format!("http://rebind.test:{}/", target.port());
    let err = client.get(&url).send().await.unwrap_err();
    assert!(err.is_connect(), "{err:?}");
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 0);
    assert_eq!(target_hits.load(Ordering::SeqCst), 0);

    // The same host, allowlisted, goes direct and never to the proxy.
    let list = Allowlist::fixed(&["rebind.test".to_string()]);
    let client = builder_with(list, Arc::new(Fixed(vec![target])), &proxies)
        .build()
        .unwrap();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 0);
    assert_eq!(target_hits.load(Ordering::SeqCst), 1);
}

#[test]
fn the_calendar_builder_takes_the_proxy_from_the_environment() {
    let _lock = crate::test_support::ENV_LOCK.blocking_lock();
    let vars = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "NO_PROXY",
        "no_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    let saved: Vec<_> = vars.iter().map(|v| (*v, std::env::var(v).ok())).collect();
    for v in vars {
        std::env::remove_var(v);
    }
    std::env::set_var("HTTPS_PROXY", "http://egress.corp:8080");
    std::env::set_var("NO_PROXY", "exchange.corp");
    let cfg = ProxyConfig::from_env();
    for (v, old) in saved {
        match old {
            Some(val) => std::env::set_var(v, val),
            None => std::env::remove_var(v),
        }
    }
    assert_eq!(cfg.https.as_deref(), Some("http://egress.corp:8080"));
    assert_eq!(cfg.no_proxy.as_deref(), Some("exchange.corp"));
    assert_eq!(cfg.hosts(), vec!["egress.corp"]);
    // client_builder() passes ProxyConfig::from_env() to builder_with.
    let src = include_str!("../net_guard.rs");
    let body = src.split("pub fn client_builder()").nth(1).unwrap();
    assert!(body[..body.find("\n}").unwrap()].contains("&ProxyConfig::from_env()"));
}

/// A server on `chain.test` answering `/N` with a redirect to `/N+1` while
/// `N < hops`, and 200 after that, counting requests.
async fn chain_server(hops: usize) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&hits);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 2048];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]);
                let step: usize = head
                    .split_whitespace()
                    .nth(1)
                    .and_then(|p| p.trim_start_matches('/').parse().ok())
                    .unwrap_or(0);
                let reply = if step < hops {
                    format!(
                        "HTTP/1.1 302 Found\r\nlocation: /{}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                        step + 1
                    )
                } else {
                    "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
                        .to_string()
                };
                let _ = sock.write_all(reply.as_bytes()).await;
            });
        }
    });
    (local, hits)
}

/// `redirects(max)` follows `max - 1` redirects and refuses the next one,
/// the count reqwest 0.11's `Policy::limited(max)` kept (0.12's follows
/// one more). The CalDAV client passes 10, so it follows 9, as before.
#[tokio::test]
async fn redirects_follow_one_fewer_than_max() {
    let list = Allowlist::fixed(&["chain.test".to_string()]);
    let get = |hops: usize| {
        let list = list.clone();
        async move {
            let (server, hits) = chain_server(hops).await;
            let client = builder_with(
                list.clone(),
                Arc::new(Fixed(vec![server])),
                &ProxyConfig::none(),
            )
            .redirect(redirects_with(3, list, Vec::new()))
            .build()
            .unwrap();
            let url = format!("http://chain.test:{}/0", server.port());
            let res = client.get(&url).send().await;
            let hits_first = hits.load(Ordering::SeqCst);
            // The count starts again with every request on the same client.
            let again = client.get(&url).send().await;
            assert_eq!(res.is_ok(), again.is_ok());
            (res, hits_first)
        }
    };
    let (ok, hits) = get(2).await;
    assert_eq!(ok.unwrap().status(), 200);
    assert_eq!(hits, 3);
    let (err, hits) = get(3).await;
    let err = err.unwrap_err();
    assert!(err.is_redirect(), "{err:?}");
    assert_eq!(hits, 3);
}

/// reqwest 0.12 accepts a proxy URL of any scheme; calrs keeps only HTTP
/// and HTTPS ones, and reads a setting without a scheme as `http://`.
#[test]
fn only_http_proxies_are_kept_and_a_bare_host_reads_as_http() {
    for raw in [
        "socks5://proxy.corp:1080",
        "socks4a://proxy.corp",
        "ftp://proxy.corp:21",
    ] {
        let cfg =
            ProxyConfig::from_vars(|name: &str| (name == "HTTPS_PROXY").then(|| raw.to_string()));
        assert!(!cfg.is_set(), "{raw}");
        assert!(cfg.ignored, "{raw}");
        assert!(cfg.hosts().is_empty(), "{raw}");
    }
    let https = ProxyConfig::from_vars(|name: &str| {
        (name == "HTTPS_PROXY").then(|| "https://Egress.corp:8443".to_string())
    });
    assert_eq!(https.hosts(), vec!["egress.corp"]);
    assert!(!https.ignored);
    let bare = ProxyConfig::from_vars(|name: &str| {
        (name == "HTTP_PROXY").then(|| "localhost:3128".to_string())
    });
    assert!(bare.is_set());
    assert_eq!(bare.hosts(), vec!["localhost"]);
}

/// The NO_PROXY forms calrs documents still send a host direct (and so
/// through the guarded resolver) with the 0.12 matcher: a domain and its
/// subdomains, case aside, and an IP network.
#[tokio::test]
async fn no_proxy_domains_and_networks_go_direct() {
    let (proxy, proxy_hits) = counting_server().await;
    let (target, target_hits) = counting_server().await;
    let proxies = ProxyConfig {
        http: Some(format!("http://{proxy}")),
        no_proxy: Some("Intranet.Test, 127.0.0.0/8".to_string()),
        ..ProxyConfig::none()
    };
    let list = Allowlist::fixed(&["intranet.test".to_string(), "cal.intranet.test".to_string()]);
    let client = builder_with(list, Arc::new(Fixed(vec![target])), &proxies)
        .build()
        .unwrap();
    for host in [
        "intranet.test".to_string(),
        "cal.intranet.test".to_string(),
        "127.0.0.1".to_string(),
    ] {
        let url = format!("http://{host}:{}/", target.port());
        assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    }
    assert_eq!(target_hits.load(Ordering::SeqCst), 3);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 0);
    // A host outside the list goes to the proxy.
    let resp = client.get("http://elsewhere.test/").send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
}

/// The URL's port decides where a connection goes, never a port the lookup
/// answered with: a URL without one goes to the scheme's default port.
#[tokio::test]
async fn the_url_port_wins_over_a_port_from_the_lookup() {
    let (server, hits) = counting_server().await;
    let list = Allowlist::fixed(&["noport.test".to_string()]);
    let client = builder_with(list, Arc::new(Fixed(vec![server])), &ProxyConfig::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let _ = client.get("http://noport.test/").send().await;
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let url = format!("http://noport.test:{}/", server.port());
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

/// The clients outside the private-host policy take proxies as the guarded
/// ones do: a SOCKS setting is dropped and the request goes direct (reqwest
/// 0.12 alone would speak plain HTTP to the SOCKS host), an HTTP proxy is
/// used, and `NO_PROXY` hosts go direct.
#[tokio::test]
async fn plain_clients_take_proxies_like_the_guarded_ones() {
    let (socks, socks_hits) = counting_server().await;
    let (target, target_hits) = counting_server().await;
    let url = format!("http://127.0.0.1:{}/", target.port());

    let cfg = ProxyConfig::from_vars(|name: &str| {
        (name == "ALL_PROXY").then(|| format!("socks5://{socks}"))
    });
    let client = plain_builder_with(&cfg).build().unwrap();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(socks_hits.load(Ordering::SeqCst), 0);
    assert_eq!(target_hits.load(Ordering::SeqCst), 1);

    let (proxy, proxy_hits) = counting_server().await;
    let cfg = ProxyConfig {
        http: Some(format!("http://{proxy}")),
        no_proxy: Some("localhost".to_string()),
        ..ProxyConfig::none()
    };
    let client = plain_builder_with(&cfg).build().unwrap();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
    assert_eq!(target_hits.load(Ordering::SeqCst), 1);
    let direct = format!("http://localhost:{}/", target.port());
    assert_eq!(client.get(&direct).send().await.unwrap().status(), 200);
    assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
    assert_eq!(target_hits.load(Ordering::SeqCst), 2);

    // And with nothing configured, nothing from reqwest's own reading.
    let client = plain_builder_with(&ProxyConfig::none()).build().unwrap();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert_eq!(target_hits.load(Ordering::SeqCst), 3);
}

/// Every HTTP client calrs builds outside tests starts from
/// [`client_builder`], [`builder_with`] or [`plain_builder`], so none reads
/// proxies through reqwest's own environment handling.
#[test]
fn no_client_is_built_outside_net_guard() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let forbidden = [
        "Client::builder()",
        "Client::new()",
        "ClientBuilder::new()",
        "reqwest::get(",
        "Client::default()",
    ];
    let mut found = Vec::new();
    for path in files {
        if path.ends_with("net_guard.rs") || path.file_name().is_some_and(|n| n == "tests.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        // Test modules build their own clients against local servers.
        let shipped = src.split("#[cfg(test)]\nmod tests").next().unwrap();
        for (n, line) in shipped.lines().enumerate() {
            if forbidden.iter().any(|f| line.contains(f)) && !line.trim_start().starts_with("//") {
                found.push(format!("{}:{}", path.display(), n + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "build clients through net_guard: {found:?}"
    );
}
