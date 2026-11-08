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
    let client = builder_with(none, lookup.clone()).build().unwrap();
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
    let client = builder_with(list, lookup).build().unwrap();
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
    let client = builder_with(Allowlist::Settings, lookup).build().unwrap();
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
fn redirects_to_a_new_private_literal_are_refused() {
    let none = Allowlist::fixed(&[]);
    let url = |s: &str| Url::parse(s).unwrap();
    let start = url("https://cal.example.com/dav/");
    assert!(redirect_refused(
        &url("http://169.254.169.254/latest/"),
        Some(&start),
        &none
    ));
    // Names are left to the resolver.
    assert!(!redirect_refused(
        &url("https://other.example.com/"),
        Some(&start),
        &none
    ));
    // A server saved as a literal may redirect to itself.
    let local = url("http://127.0.0.1:5232/");
    assert!(!redirect_refused(
        &url("http://127.0.0.1:5232/dav/"),
        Some(&local),
        &none
    ));
    let list = Allowlist::fixed(&["10.0.0.5".to_string()]);
    assert!(!redirect_refused(
        &url("http://10.0.0.5/"),
        Some(&start),
        &list
    ));
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
    let client = builder_with(list.clone(), Arc::new(Fixed(vec![local])))
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
