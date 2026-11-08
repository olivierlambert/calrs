//! Global Exchange (EWS) impersonation: the admin card, the lock on user
//! sources, managed sources, and who gets provisioned.

use super::*;

const EWS_URL: &str = "https://example.com/EWS/Exchange.asmx";

/// Store a complete global Exchange config, before the router loads it.
async fn store_global_config(pool: &SqlitePool, lock: bool, auto_provision: bool) {
    let enc = crate::crypto::encrypt_value(&[0u8; 32], "svc-pass").unwrap();
    sqlx::query(
        "UPDATE auth_config SET ews_global_enabled = 1, ews_global_url = ?, \
             ews_service_username = 'svc@example.com', ews_service_password_enc = ?, \
             ews_lock_user_sources = ?, ews_auto_provision = ? WHERE id = 'singleton'",
    )
    .bind(EWS_URL)
    .bind(&enc)
    .bind(lock as i64)
    .bind(auto_provision as i64)
    .execute(pool)
    .await
    .unwrap();
}

/// An app whose global Exchange config is on, plus a regular (non-admin)
/// user with a session and an account. Returns (app, pool, admin session,
/// user id, user session).
async fn setup_with_global(
    lock: bool,
    auto_provision: bool,
) -> (Router, SqlitePool, String, String, String) {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_global_config(&pool, lock, auto_provision).await;

    let admin_session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&admin_session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let (user_id, user_session) =
        insert_test_user_with_session(&pool, "Alice", "alice@example.com", "alice").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Alice', 'alice@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    (app, pool, admin_session, user_id, user_session)
}

async fn managed_source_of(pool: &SqlitePool, user_id: &str) -> Option<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT cs.id, cs.impersonate_email FROM caldav_sources cs \
         JOIN accounts a ON a.id = cs.account_id \
         WHERE a.user_id = ? AND cs.managed = 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Mark a user's address as trusted, as `calrs user create` or a verified
/// OIDC login does.
async fn trust(pool: &SqlitePool, user_id: &str) {
    sqlx::query("UPDATE users SET email_trusted = 1 WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Record that the identity provider verified the user's address, as an SSO
/// login with `email_verified` does. A new domain override re-maps such an
/// address without asking the admin to confirm it again.
async fn idp_verify(pool: &SqlitePool, user_id: &str) {
    sqlx::query("UPDATE users SET email_trusted = 1, email_verified_by_idp = 1 WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Give `user_id` the trusted address `email` (as an SSO login reporting a
/// verified address does) and provision them.
async fn provision(pool: &SqlitePool, user_id: &str, email: &str) -> String {
    sqlx::query("UPDATE users SET email = ?, email_trusted = 1 WHERE id = ?")
        .bind(email)
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
    let cfg = crate::ews::global::load_ews_global_config(pool, &[0u8; 32])
        .await
        .expect("config is complete");
    crate::ews::global::provision_managed_ews_source_for_user(pool, &cfg, user_id)
        .await
        .unwrap();
    managed_source_of(pool, user_id).await.unwrap().0
}

#[tokio::test]
async fn locked_sources_refuse_new_sources_to_users_but_not_admins() {
    let (app, _, admin_session, _, user_session) = setup_with_global(true, false).await;

    let user = app
        .clone()
        .oneshot(get_authed("/dashboard/sources/new", &user_session))
        .await
        .unwrap();
    assert_eq!(user.status(), 403);

    let csrf = "csrf-lock";
    let created = app
        .clone()
        .oneshot(post_form(
            "/dashboard/sources/new",
            &user_session,
            csrf,
            &format!("_csrf={csrf}&name=Mine&url=https%3A%2F%2Fexample.com%2Fdav%2F&username=a&password=p&no_test=on"),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), 403);

    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &user_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(!page.contains("/dashboard/sources/new\""), "no Add button");

    let admin = app
        .oneshot(get_authed("/dashboard/sources/new", &admin_session))
        .await
        .unwrap();
    assert_eq!(admin.status(), 200);
}

#[tokio::test]
async fn a_managed_source_cannot_be_edited_or_removed_by_its_user() {
    let (app, pool, admin_session, user_id, user_session) = setup_with_global(false, false).await;
    let sid = provision(&pool, &user_id, "alice@example.com").await;

    let edit = app
        .clone()
        .oneshot(get_authed(
            &format!("/dashboard/sources/{sid}/edit"),
            &user_session,
        ))
        .await
        .unwrap();
    assert_eq!(edit.status(), 403);

    let csrf = "csrf-managed";
    let removed = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/sources/{sid}/remove"),
            &user_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(removed.status(), 403);
    assert!(managed_source_of(&pool, &user_id).await.is_some());

    // The page shows the badge and no Edit/Remove for it.
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &user_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("Admin-managed"));
    assert!(!page.contains(&format!("/dashboard/sources/{sid}/edit")));
    assert!(!page.contains(&format!("/dashboard/sources/{sid}/remove")));

    // The admin sees it in the per-user status table.
    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("alice@example.com"),
        "listed in the status table"
    );
}

#[tokio::test]
async fn admin_save_refuses_an_incomplete_config_and_provisions_a_complete_one() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    trust(&pool, &admin_id).await;
    idp_verify(&pool, &admin_id).await;
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let csrf = "csrf-ews-admin";
    let url = "https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx";

    // Enabled without a password: refused, nothing stored.
    let resp = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username=svc"
            ),
        ))
        .await
        .unwrap();
    let location = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("ews=incomplete"), "{location}");
    let enabled: i64 =
        sqlx::query_scalar("SELECT ews_global_enabled FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(enabled, 0);

    // Complete, with "provision now": saved and every enabled user provisioned.
    let resp = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username=svc&ews_service_password=secret&ews_impersonation_from_domain=example.com&ews_impersonation_domain=%40corp.example&provision_now=1"),
        ))
        .await
        .unwrap();
    let location = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(
        location.contains("ews=saved&n=1"),
        "the redirect carries how many sources the batch created: {location}"
    );
    let (mailbox,): (Option<String>,) = sqlx::query_as(
        "SELECT impersonate_email FROM caldav_sources WHERE managed = 1 AND account_id IN \
         (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        mailbox.as_deref(),
        Some("test@corp.example"),
        "the mailbox actually impersonated is stored"
    );

    // The card shows the address actually impersonated, after the override,
    // and the password is kept when the field is left empty.
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin?ews=saved&n=1", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("test@corp.example"));
    assert!(page.contains("Exchange settings saved. Managed sources created: 1."));
    app.oneshot(post_form(
        "/dashboard/admin/ews",
        &session,
        csrf,
        &format!(
            "_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username=svc&ews_impersonation_from_domain=example.com&ews_impersonation_domain=corp.example"
        ),
    ))
    .await
    .unwrap();
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .expect("still complete");
    assert_eq!(cfg.service_password, "secret");
    assert_eq!(
        cfg.domain_override,
        Some(crate::ews::global::DomainOverride {
            from: "example.com".to_string(),
            to: "corp.example".to_string()
        })
    );
}

/// Self-registration does not prove the address belongs to the person, and
/// a managed source would open that address's mailbox to them: not at
/// sign-up, not by the admin's batch, not by "provision now".
#[tokio::test]
async fn self_registration_is_not_provisioned() {
    let (app, pool, admin_session, _, _) = setup_with_global(false, true).await;
    sqlx::query("UPDATE auth_config SET registration_enabled = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-register-ews";
    let resp = app
        .clone()
        .oneshot(post_form_unauthed(
            "/auth/register",
            csrf,
            &format!("_csrf={csrf}&name=Boss&email=ceo%40example.com&password=strongpassword123"),
        ))
        .await
        .unwrap();
    assert!(resp.status().is_redirection());
    let user_id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'ceo@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(managed_source_of(&pool, &user_id).await.is_none());

    // The admin saves the card with auto-provision on and asks for
    // "provision now": the impostor still gets nothing, and is listed among
    // the addresses to confirm instead.
    let csrf = "csrf-batch";
    let resp = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &admin_session,
            csrf,
            &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url=https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx&ews_service_username=svc%40example.com&ews_auto_provision=on&provision_now=1"),
        ))
        .await
        .unwrap();
    assert!(resp.headers()["location"]
        .to_str()
        .unwrap()
        .contains("ews=saved"));
    assert!(managed_source_of(&pool, &user_id).await.is_none());
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("Addresses to confirm"));
    assert!(page.contains(&format!("value=\"{user_id}\"")));

    assert!(page.contains("name=\"email\" value=\"ceo@example.com\""));

    // A confirmation covers what the admin saw: an address that changed
    // since (here the form shows an older one), or a mailbox the override
    // now maps elsewhere, confirms nothing and asks to look again.
    for body in [
        format!("_csrf={csrf}&user_id={user_id}&email=old%40example.com&mailbox=old%40example.com"),
        format!(
            "_csrf={csrf}&user_id={user_id}&email=ceo%40example.com&mailbox=ceo%40corp.example"
        ),
    ] {
        let resp = app
            .clone()
            .oneshot(post_form(
                "/dashboard/admin/ews/trust",
                &admin_session,
                csrf,
                &body,
            ))
            .await
            .unwrap();
        assert!(resp.headers()["location"]
            .to_str()
            .unwrap()
            .contains("ews=trust-changed"));
        assert!(managed_source_of(&pool, &user_id).await.is_none());
    }

    // Confirming it is the admin's call, and provisions right away.
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews/trust",
            &admin_session,
            csrf,
            &format!(
                "_csrf={csrf}&user_id={user_id}&email=ceo%40example.com&mailbox=ceo%40example.com"
            ),
        ))
        .await
        .unwrap();
    assert!(resp.headers()["location"]
        .to_str()
        .unwrap()
        .contains("ews=trusted"));
    assert_eq!(
        managed_source_of(&pool, &user_id)
            .await
            .unwrap()
            .1
            .as_deref(),
        Some("ceo@example.com")
    );
}

/// Exchange matches addresses without regard to case, so the "email taken"
/// check at registration must too: `CEO@example.com` is `ceo@example.com`.
#[tokio::test]
async fn registration_refuses_a_case_variant_of_an_existing_address() {
    let (app, pool, _, _, _) = setup_with_global(false, true).await;
    sqlx::query("UPDATE auth_config SET registration_enabled = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-register-case";
    let resp = app
        .oneshot(post_form_unauthed(
            "/auth/register",
            csrf,
            &format!("_csrf={csrf}&name=Imp&email=Alice%40Example.COM&password=strongpassword123"),
        ))
        .await
        .unwrap();
    assert!(!resp.status().is_redirection());
    let page = body_string(resp).await;
    assert!(
        page.contains(&crate::i18n::translate(
            "en",
            "auth-error-email-taken",
            None
        )),
        "{page}"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE lower(email) = 'alice@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn provisioning_again_follows_a_changed_address() {
    let (_, pool, _, user_id, _) = setup_with_global(false, true).await;
    let first = provision(&pool, &user_id, "alice@example.com").await;
    let second = provision(&pool, &user_id, "alice.smith@example.com").await;
    assert_eq!(first, second, "still one managed source");
    let (_, mailbox) = managed_source_of(&pool, &user_id).await.unwrap();
    assert_eq!(mailbox.as_deref(), Some("alice.smith@example.com"));
}

/// With global Exchange switched off, a managed source has nothing to reach:
/// its sync is skipped, not recorded as a failure that would email the user.
#[tokio::test]
async fn a_managed_source_is_skipped_while_global_exchange_is_off() {
    let (_, pool, _, user_id, _) = setup_with_global(false, false).await;
    let sid = provision(&pool, &user_id, "alice@example.com").await;
    sqlx::query("UPDATE auth_config SET ews_global_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();

    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &sid).await;

    let (error, failures): (Option<String>, i64) = sqlx::query_as(
        "SELECT last_sync_error, sync_failure_count FROM caldav_sources WHERE id = ?",
    )
    .bind(&sid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((error, failures), (None, 0));
}

// --- Functional end-to-end: a mock Exchange on loopback ---

/// One request the mock Exchange answered.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EwsCall {
    op: String,
    /// The mailbox of the `t:ExchangeImpersonation` header, if any.
    impersonate: Option<String>,
    /// The `X-AnchorMailbox` header, if any.
    anchor: Option<String>,
    /// The Basic-auth principal.
    user: Option<String>,
}

/// A tiny Exchange on loopback, speaking just enough EWS for a sync and a
/// write-back. Every mailbox has its own calendar folder (`CAL-<local part>`)
/// holding one busy event, so a test can tell whose calendar an item came
/// from. Records every call with the mailbox it was impersonating.
struct MockEws {
    calls: std::sync::Mutex<Vec<EwsCall>>,
    /// The busy event each mailbox reports, as compact UTC iCal strings.
    busy: std::sync::Mutex<Option<(String, String)>>,
    /// Items created through `CreateItem`: item id -> (folder, decoded ICS).
    created: std::sync::Mutex<std::collections::HashMap<String, (String, String)>>,
    /// Mailboxes answered with `ErrorNonExistentMailbox`.
    no_mailbox: std::sync::Mutex<Vec<String>>,
}

impl MockEws {
    fn calls(&self) -> Vec<EwsCall> {
        self.calls.lock().unwrap().clone()
    }
    fn ops_for(&self, mailbox: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.impersonate.as_deref() == Some(mailbox))
            .map(|c| c.op)
            .collect()
    }
    /// The (folder, ICS) of each item the calendar holds.
    fn created(&self) -> Vec<(String, String)> {
        self.created.lock().unwrap().values().cloned().collect()
    }
    fn set_busy(&self, start: &str, end: &str) {
        *self.busy.lock().unwrap() = Some((start.to_string(), end.to_string()));
    }
    fn refuse_mailbox(&self, mailbox: &str) {
        self.no_mailbox.lock().unwrap().push(mailbox.to_string());
    }
}

fn soap_body(inner: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         xmlns:m=\"{m}\" xmlns:t=\"{t}\"><s:Body>{inner}</s:Body></s:Envelope>",
        m = crate::ews::soap::NS_MESSAGES,
        t = crate::ews::soap::NS_TYPES,
    )
}

/// The value of `<t:Tag>` in a request body, or of an attribute like
/// `Value="..."` when `attr` is set.
fn xml_value(body: &str, tag: &str, attr: Option<&str>) -> Option<String> {
    match attr {
        None => {
            // The tag may carry attributes (`<t:MimeContent CharacterSet=..>`).
            let i = body.find(&format!("<{tag}"))?;
            let i = i + body[i..].find('>')? + 1;
            let j = body[i..].find(&format!("</{tag}>"))?;
            Some(body[i..i + j].to_string())
        }
        Some(name) => {
            let i = body.find(&format!("<{tag} "))?;
            let rest = &body[i..];
            let k = rest.find(&format!("{name}=\""))? + name.len() + 2;
            let end = rest[k..].find('"')?;
            Some(rest[k..k + end].to_string())
        }
    }
}

async fn spawn_mock_ews() -> (String, Arc<MockEws>) {
    use axum::http::{HeaderMap as AxHeaderMap, StatusCode};
    let ews = Arc::new(MockEws {
        calls: std::sync::Mutex::new(Vec::new()),
        busy: std::sync::Mutex::new(None),
        created: std::sync::Mutex::new(std::collections::HashMap::new()),
        no_mailbox: std::sync::Mutex::new(Vec::new()),
    });
    let state = ews.clone();
    let app = Router::new().fallback(move |headers: AxHeaderMap, body: String| {
        let ews = state.clone();
        async move {
            let op = [
                "GetFolder",
                "FindFolder",
                "FindItem",
                "CreateItem",
                "DeleteItem",
            ]
            .into_iter()
            .find(|op| body.contains(&format!("<m:{op}")))
            .unwrap_or("Unknown")
            .to_string();
            let impersonate = xml_value(&body, "t:SmtpAddress", None);
            let anchor = headers
                .get("X-AnchorMailbox")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            // Decode the Basic credentials so a test can check which account
            // authenticated (the service account, never the user).
            let user = headers.get("authorization").and_then(|v| {
                use base64::Engine;
                let raw = v.to_str().ok()?.strip_prefix("Basic ")?;
                let decoded = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
                let text = String::from_utf8(decoded).ok()?;
                Some(text.split(':').next()?.to_string())
            });
            ews.calls.lock().unwrap().push(EwsCall {
                op: op.clone(),
                impersonate: impersonate.clone(),
                anchor,
                user,
            });

            let mailbox = impersonate.clone().unwrap_or_default();
            if ews.no_mailbox.lock().unwrap().contains(&mailbox) {
                // What Exchange answers for an address with no mailbox: a
                // SOAP fault whose detail carries the response code.
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    soap_body(
                        "<s:Fault><faultcode>a:ErrorNonExistentMailbox</faultcode>\
                         <faultstring>The SMTP address has no mailbox associated with it.\
                         </faultstring><detail>\
                         <e:ResponseCode>ErrorNonExistentMailbox</e:ResponseCode>\
                         </detail></s:Fault>",
                    ),
                );
            }
            let local = mailbox.split('@').next().unwrap_or("none").to_string();
            let folder = format!("CAL-{local}");
            let inner = match op.as_str() {
                "GetFolder" => {
                    // The well-known calendar folder is the mailbox's default
                    // calendar; anything else asked for is the inbox probe.
                    let id = if body.contains("DistinguishedFolderId Id=\"calendar\"") {
                        folder.clone()
                    } else {
                        format!("inbox-{local}")
                    };
                    format!(
                        "<m:GetFolderResponse><m:ResponseMessages><m:GetFolderResponseMessage \
                         ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>\
                         <m:Folders><t:Folder><t:FolderId Id=\"{id}\"/></t:Folder>\
                         </m:Folders></m:GetFolderResponseMessage></m:ResponseMessages>\
                         </m:GetFolderResponse>"
                    )
                }
                "FindFolder" => format!(
                    "<m:FindFolderResponse><m:ResponseMessages><m:FindFolderResponseMessage \
                     ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>\
                     <m:RootFolder TotalItemsInView=\"1\"><t:Folders><t:CalendarFolder>\
                     <t:FolderId Id=\"{folder}\" ChangeKey=\"CK\"/>\
                     <t:DisplayName>Calendar</t:DisplayName></t:CalendarFolder></t:Folders>\
                     </m:RootFolder></m:FindFolderResponseMessage></m:ResponseMessages>\
                     </m:FindFolderResponse>"
                ),
                "FindItem" => {
                    // Two shapes: a CalendarView snapshot of the window, and a
                    // lookup of one UID (used before a write and a delete).
                    let items = if let Some(uid) = xml_value(&body, "t:Constant", Some("Value")) {
                        ews.created
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|(_, (f, ics))| *f == folder && ics.contains(&uid))
                            .map(|(id, _)| {
                                format!(
                                    "<t:CalendarItem><t:ItemId Id=\"{id}\" ChangeKey=\"CK\"/>\
                                         <t:UID>{uid}</t:UID></t:CalendarItem>"
                                )
                            })
                            .collect::<String>()
                    } else {
                        match ews.busy.lock().unwrap().clone() {
                            Some((start, end)) => format!(
                                "<t:CalendarItem><t:ItemId Id=\"EV-{local}\" ChangeKey=\"CK\"/>\
                                 <t:UID>busy-{local}</t:UID>\
                                 <t:Subject>Busy of {local}</t:Subject>\
                                 <t:Start>{start}</t:Start><t:End>{end}</t:End>\
                                 <t:IsAllDayEvent>false</t:IsAllDayEvent>\
                                 <t:IsCancelled>false</t:IsCancelled>\
                                 <t:LegacyFreeBusyStatus>Busy</t:LegacyFreeBusyStatus>\
                                 </t:CalendarItem>"
                            ),
                            None => String::new(),
                        }
                    };
                    format!(
                        "<m:FindItemResponse><m:ResponseMessages><m:FindItemResponseMessage \
                         ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>\
                         <m:RootFolder TotalItemsInView=\"1\" IncludesLastItemInRange=\"true\">\
                         <t:Items>{items}</t:Items></m:RootFolder>\
                         </m:FindItemResponseMessage></m:ResponseMessages></m:FindItemResponse>"
                    )
                }
                "CreateItem" => {
                    use base64::Engine;
                    let mime = xml_value(&body, "t:MimeContent", None).unwrap_or_default();
                    let ics = base64::engine::general_purpose::STANDARD
                        .decode(mime)
                        .ok()
                        .and_then(|b| String::from_utf8(b).ok())
                        .unwrap_or_default();
                    let target = xml_value(&body, "t:FolderId", Some("Id")).unwrap_or_default();
                    let id = format!("NEW-{}", uuid::Uuid::new_v4());
                    ews.created
                        .lock()
                        .unwrap()
                        .insert(id.clone(), (target, ics));
                    format!(
                        "<m:CreateItemResponse><m:ResponseMessages><m:CreateItemResponseMessage \
                         ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>\
                         <m:Items><t:CalendarItem><t:ItemId Id=\"{id}\" ChangeKey=\"CK\"/>\
                         </t:CalendarItem></m:Items></m:CreateItemResponseMessage>\
                         </m:ResponseMessages></m:CreateItemResponse>"
                    )
                }
                "DeleteItem" => {
                    if let Some(id) = xml_value(&body, "t:ItemId", Some("Id")) {
                        ews.created.lock().unwrap().remove(&id);
                    }
                    "<m:DeleteItemResponse><m:ResponseMessages><m:DeleteItemResponseMessage \
                     ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>\
                     </m:DeleteItemResponseMessage></m:ResponseMessages></m:DeleteItemResponse>"
                        .to_string()
                }
                _ => "<m:Unknown/>".to_string(),
            };
            (StatusCode::OK, soap_body(&inner))
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://127.0.0.1:{port}/EWS/Exchange.asmx"), ews)
}

/// The slot start times the page offers for `date`, read from the JSON the
/// slot picker renders from (the times themselves are drawn client-side).
fn slot_times_on(page: &str, date: &str) -> Vec<String> {
    let key = format!("\"{date}\": [");
    let Some(i) = page.find(&key) else {
        return Vec::new();
    };
    let rest = &page[i + key.len()..];
    let end = rest.find(']').unwrap_or(0);
    rest[..end]
        .split("\"start\":\"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next().map(str::to_string))
        .collect()
}

/// Store a global config pointing at `endpoint`, with the mailbox-domain
/// override `(from, to)` when set.
async fn store_config_for(pool: &SqlitePool, endpoint: &str, domain: Option<(&str, &str)>) {
    let enc = crate::crypto::encrypt_value(&[0u8; 32], "svc-pass").unwrap();
    sqlx::query(
        "UPDATE auth_config SET ews_global_enabled = 1, ews_global_url = ?, \
             ews_service_username = 'svc@example.com', ews_service_password_enc = ?, \
             ews_auto_provision = 1, ews_impersonation_from_domain = ?, \
             ews_impersonation_domain = ? WHERE id = 'singleton'",
    )
    .bind(endpoint)
    .bind(&enc)
    .bind(domain.map(|d| d.0))
    .bind(domain.map(|d| d.1))
    .execute(pool)
    .await
    .unwrap();
}

/// The whole point of the feature: a managed source, with no credentials of
/// its own, reads the user's own Exchange calendar through the service
/// account, and what it finds blocks that user's booking slots.
#[tokio::test]
async fn a_managed_source_syncs_the_users_mailbox_and_blocks_their_slots() {
    let pool = setup_test_db().await;
    let (host_id, account_id, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    // The login domain differs from the mailbox domain, as on an AD where the
    // UPN is not the SMTP address.
    store_config_for(&pool, &endpoint, Some(("example.com", "corp.example"))).await;

    // A meeting on the host's Exchange calendar, on the next Monday at 10:00.
    let now = Utc::now().naive_utc();
    let mut monday = now.date() + Duration::days(1);
    while monday.weekday() != chrono::Weekday::Mon {
        monday += Duration::days(1);
    }
    ews.set_busy(
        &monday.format("%Y-%m-%dT10:00:00Z").to_string(),
        &monday.format("%Y-%m-%dT11:00:00Z").to_string(),
    );

    let source_id = provision(&pool, &host_id, "test@example.com").await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;

    // The service account authenticated, and impersonated the host's mailbox
    // after the domain override, with the anchor header DAG routing needs.
    let calls = ews.calls();
    assert!(!calls.is_empty(), "the mock was reached");
    for call in &calls {
        assert_eq!(call.user.as_deref(), Some("svc@example.com"), "{call:?}");
        assert_eq!(
            call.impersonate.as_deref(),
            Some("test@corp.example"),
            "{call:?}"
        );
        assert_eq!(
            call.anchor.as_deref(),
            Some("test@corp.example"),
            "{call:?}"
        );
    }

    // The event landed on a calendar of this source, and nothing failed. A
    // managed source keeps busy times only: the meeting's subject (and any
    // description, location or iCal text) is not stored.
    #[allow(clippy::type_complexity)]
    let (uid, summary, description, location, raw_ical, error): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT e.uid, e.summary, e.description, e.location, e.raw_ical, \
                (SELECT last_sync_error FROM caldav_sources WHERE id = ?) \
         FROM events e JOIN calendars c ON c.id = e.calendar_id WHERE c.source_id = ?",
    )
    .bind(&source_id)
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(uid, "busy-test");
    assert_eq!(
        (summary, description, location, raw_ical),
        (None, None, None, None)
    );
    assert_eq!(error, None);

    // And it blocks the host's availability: the slot is gone from the page a
    // guest sees.
    let busy = fetch_busy_times_for_user(
        &pool,
        &host_id,
        monday.and_hms_opt(0, 0, 0).unwrap(),
        monday.and_hms_opt(23, 59, 0).unwrap(),
        Tz::UTC,
        Some(&et_id),
    )
    .await;
    assert_eq!(
        busy,
        vec![(
            monday.and_hms_opt(10, 0, 0).unwrap(),
            monday.and_hms_opt(11, 0, 0).unwrap()
        )],
        "the Exchange meeting blocks the host's time"
    );

    let date = monday.format("%Y-%m-%d").to_string();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let page = body_string(
        app.oneshot(get(&format!("/u/testuser/test-meeting?date={date}&tz=UTC")))
            .await
            .unwrap(),
    )
    .await;
    let offered = slot_times_on(&page, &date);
    assert!(
        offered.contains(&"09:00".to_string()),
        "the morning is still bookable: {offered:?}"
    );
    assert!(
        !offered.contains(&"10:00".to_string()) && !offered.contains(&"10:30".to_string()),
        "the Exchange meeting's hour is not offered: {offered:?}"
    );
    // The account the event belongs to is the host's own.
    let owner: String =
        sqlx::query_scalar("SELECT cs.account_id FROM caldav_sources cs WHERE cs.id = ?")
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner, account_id);
}

/// Impersonation reads one mailbox per user, so the one thing that must never
/// happen is a user seeing someone else's meetings.
#[tokio::test]
async fn each_user_only_gets_their_own_mailbox() {
    let pool = setup_test_db().await;
    let (alice, _, _) = seed_test_data(&pool).await;
    let bob = insert_role_user(&pool, "bob@example.com", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Bob', 'bob@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let day = (Utc::now().naive_utc().date() + Duration::days(3))
        .format("%Y-%m-%dT14:00:00Z")
        .to_string();
    let end = day.replace("T14:", "T15:");
    ews.set_busy(&day, &end);

    let alice_source = provision(&pool, &alice, "test@example.com").await;
    let bob_source = provision(&pool, &bob, "bob@example.com").await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &alice_source).await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &bob_source).await;

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT u.email, e.uid FROM events e \
         JOIN calendars c ON c.id = e.calendar_id \
         JOIN caldav_sources cs ON cs.id = c.source_id \
         JOIN accounts a ON a.id = cs.account_id \
         JOIN users u ON u.id = a.user_id ORDER BY u.email",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("bob@example.com".to_string(), "busy-bob".to_string()),
            ("test@example.com".to_string(), "busy-test".to_string()),
        ],
        "each user sees only their own mailbox"
    );
    assert!(ews
        .ops_for("bob@example.com")
        .contains(&"FindFolder".to_string()));
    assert!(ews
        .ops_for("test@example.com")
        .contains(&"FindFolder".to_string()));
}

/// A confirmed booking is written into the host's own Exchange calendar
/// through the service account, and removed from it on cancellation.
#[tokio::test]
async fn a_booking_is_written_to_the_managed_calendar_and_removed_on_cancel() {
    let pool = setup_test_db().await;
    let (host_id, _, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let source_id = provision(&pool, &host_id, "test@example.com").await;
    // Discover the calendars, then pick one to write bookings to, the way a
    // user does from the sources page.
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    let href: String = sqlx::query_scalar("SELECT href FROM calendars WHERE source_id = ?")
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(href, "CAL-test");
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = ? WHERE id = ?")
        .bind(&href)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@example.com",
        Utc::now() + Duration::days(7),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{booking_id}");

    // Exactly what a booking handler does on confirmation.
    let (details, _) = writeback::writeback_details(&pool, &uid).await.unwrap();
    caldav_push_booking(&pool, &[0u8; 32], &host_id, &uid, &details).await;

    let created = ews.created();
    assert_eq!(created.len(), 1, "one item created, got {created:?}");
    let (folder, ics) = &created[0];
    assert_eq!(folder, "CAL-test", "written to the host's own calendar");
    assert!(ics.contains(&uid), "the booking's own UID");
    assert!(
        ews.calls().iter().any(|c| c.op == "CreateItem"
            && c.impersonate.as_deref() == Some("test@example.com")
            && c.user.as_deref() == Some("svc@example.com")),
        "created as the host, authenticated as the service account"
    );

    caldav_delete_booking(&pool, &[0u8; 32], &host_id, &uid).await;
    assert!(
        ews.created().is_empty(),
        "cancelling removes it from Exchange"
    );
}

/// A mailbox the service account cannot reach fails that source alone: the
/// others still sync, and the admin panel says which one and why.
#[tokio::test]
async fn a_user_without_a_mailbox_fails_alone_and_is_named_in_the_panel() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    let bob = insert_role_user(&pool, "bob@example.com", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Bob', 'bob@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bob)
        .execute(&pool)
        .await
        .unwrap();
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    ews.refuse_mailbox("test@example.com");
    let day = (Utc::now().naive_utc().date() + Duration::days(2))
        .format("%Y-%m-%dT08:00:00Z")
        .to_string();
    ews.set_busy(&day, &day.replace("T08:", "T09:"));
    let admin_source = provision(&pool, &admin_id, "test@example.com").await;
    let bob_source = provision(&pool, &bob, "bob@example.com").await;

    // `calrs sync` exits non-zero when a source failed, and keeps going.
    assert!(crate::commands::sync::run(&pool, &[0u8; 32], false)
        .await
        .is_err());
    let errors: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, last_sync_error FROM caldav_sources ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let admin_error = errors
        .iter()
        .find(|(id, _)| *id == admin_source)
        .unwrap()
        .1
        .clone()
        .expect("the unreachable mailbox is recorded");
    assert!(
        admin_error.contains("ErrorNonExistentMailbox"),
        "the message carries the language-independent fault code: {admin_error}"
    );
    assert_eq!(
        errors.iter().find(|(id, _)| *id == bob_source).unwrap().1,
        None,
        "the other mailbox synced"
    );

    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("1 of 2 managed mailboxes syncing"));
    assert!(
        page.contains("No Exchange mailbox"),
        "the failure is explained, not dumped as a SOAP fault"
    );
}

/// Exchange folder ids belong to one mailbox, so when a source starts
/// impersonating another address, what it cached from the old mailbox must go
/// with it: kept around, those events would block the user's availability for
/// ever, and the write calendar would send bookings into a mailbox this
/// source no longer reads.
#[tokio::test]
async fn following_a_new_address_drops_what_the_old_mailbox_cached() {
    let pool = setup_test_db().await;
    let (host_id, _, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let day = Utc::now().naive_utc().date() + Duration::days(4);
    ews.set_busy(
        &day.format("%Y-%m-%dT10:00:00Z").to_string(),
        &day.format("%Y-%m-%dT11:00:00Z").to_string(),
    );

    let source_id = provision(&pool, &host_id, "test@example.com").await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = 'CAL-test' WHERE id = ?")
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();
    let cached: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events e JOIN calendars c ON c.id = e.calendar_id \
         WHERE c.source_id = ?",
    )
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cached, 1, "precondition: the old mailbox was cached");

    // The IdP now reports another address for the same user.
    provision(&pool, &host_id, "t.user@example.com").await;

    let (calendars, events, write_href): (i64, i64, Option<String>) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM calendars WHERE source_id = ?), \
                (SELECT COUNT(*) FROM events e JOIN calendars c ON c.id = e.calendar_id \
                 WHERE c.source_id = ?), \
                (SELECT write_calendar_href FROM caldav_sources WHERE id = ?)",
    )
    .bind(&source_id)
    .bind(&source_id)
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((calendars, events, write_href), (0, 0, None));
    let busy = fetch_busy_times_for_user(
        &pool,
        &host_id,
        day.and_hms_opt(0, 0, 0).unwrap(),
        day.and_hms_opt(23, 59, 0).unwrap(),
        Tz::UTC,
        Some(&et_id),
    )
    .await;
    assert!(busy.is_empty(), "the old mailbox stops blocking the host");

    // The new mailbox is read on the next sync.
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    let (href, uid): (String, String) = sqlx::query_as(
        "SELECT c.href, e.uid FROM events e JOIN calendars c ON c.id = e.calendar_id \
         WHERE c.source_id = ?",
    )
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((href.as_str(), uid.as_str()), ("CAL-t.user", "busy-t.user"));
}

/// The mailbox-domain override decides which mailbox every managed source
/// reads, so changing it has to reset them all the same way. Saving the card
/// needs a publicly resolvable endpoint (the SSRF guard refuses loopback), so
/// this one seeds the cache rather than syncing a mock.
#[tokio::test]
async fn changing_the_mailbox_domain_resets_every_managed_source() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    let source_id = provision(&pool, &admin_id, "test@example.com").await;
    // An SSO user: the override re-maps their verified address without a
    // new confirmation.
    idp_verify(&pool, &admin_id).await;
    let cal_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO calendars (id, source_id, href, display_name) VALUES (?, ?, 'CAL-old', 'Calendar')")
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at) VALUES (?, ?, 'old-uid', 'Old', '20300101T100000', '20300101T110000')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&cal_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = 'CAL-old' WHERE id = ?")
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let csrf = "csrf-domain";
    let url = "https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx";
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username=svc%40example.com&ews_impersonation_from_domain=example.com&ews_impersonation_domain=corp.example"),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.headers()["location"].to_str().unwrap(),
        "/dashboard/admin?ews=saved#ews-global"
    );
    assert_eq!(
        managed_source_of(&pool, &admin_id).await.unwrap(),
        (source_id.clone(), Some("test@corp.example".to_string())),
        "the same source now reads the mailbox the override maps to"
    );

    let (calendars, events, write_href): (i64, i64, Option<String>) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM calendars WHERE source_id = ?), \
                (SELECT COUNT(*) FROM events WHERE calendar_id = ?), \
                (SELECT write_calendar_href FROM caldav_sources WHERE id = ?)",
    )
    .bind(&source_id)
    .bind(&cal_id)
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (calendars, events, write_href),
        (0, 0, None),
        "the old mailbox's calendars went with the domain change"
    );
}

/// What a user does from the sources page with a source they did not create:
/// sync it, test it, and choose which of its calendars bookings are written
/// to. Only editing and removing are theirs to leave alone.
#[tokio::test]
async fn a_user_can_sync_test_and_pick_a_write_calendar_on_a_managed_source() {
    let pool = setup_test_db().await;
    let (host_id, _, _) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let source_id = provision(&pool, &host_id, "test@example.com").await;
    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&host_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let csrf = "csrf-managed-actions";

    let synced = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/sources/{source_id}/sync"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(synced.status().is_success() || synced.status().is_redirection());

    let tested = body_string(
        app.clone()
            .oneshot(post_form(
                &format!("/dashboard/sources/{source_id}/test"),
                &session,
                csrf,
                &format!("_csrf={csrf}"),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(tested.contains("connection OK"), "{tested}");
    assert!(
        ews.calls()
            .iter()
            .any(|c| c.op == "GetFolder" && c.impersonate.as_deref() == Some("test@example.com")),
        "the test reached the user's own mailbox"
    );

    let picked = app
        .clone()
        .oneshot(post_form(
            &format!("/dashboard/sources/{source_id}/write-calendar"),
            &session,
            csrf,
            &format!("_csrf={csrf}&calendar_href=CAL-test"),
        ))
        .await
        .unwrap();
    assert!(picked.status().is_redirection());
    let href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        href.as_deref(),
        Some("CAL-test"),
        "the host chooses where their bookings are written"
    );

    // But only among the source's own calendars: any other folder id (here
    // another mailbox's) is refused and the choice stays as it was.
    let foreign = app
        .oneshot(post_form(
            &format!("/dashboard/sources/{source_id}/write-calendar"),
            &session,
            csrf,
            &format!("_csrf={csrf}&calendar_href=CAL-ceo"),
        ))
        .await
        .unwrap();
    assert!(foreign.status().is_redirection());
    let href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        href.as_deref(),
        Some("CAL-test"),
        "a foreign href is refused"
    );
}

/// The domain override keeps an address's local part, so applied to every
/// address it would map `jsmith@subsidiary.example` onto
/// `jsmith@corp.example`: it needs the users' own domain, and rewrites only
/// addresses in exactly that domain.
#[tokio::test]
async fn the_domain_override_only_rewrites_its_from_domain() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    trust(&pool, &admin_id).await;
    idp_verify(&pool, &admin_id).await;
    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let csrf = "csrf-override";
    let base = format!(
        "_csrf={csrf}&ews_global_enabled=on&ews_global_url=https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx&ews_service_username=svc&ews_service_password=p"
    );

    // The mailbox domain alone, or a domain that is not one: refused,
    // nothing stored.
    for extra in [
        "&ews_impersonation_domain=corp.example",
        "&ews_impersonation_from_domain=example.com",
        "&ews_impersonation_from_domain=a%40example.com&ews_impersonation_domain=corp.example",
    ] {
        let resp = app
            .clone()
            .oneshot(post_form(
                "/dashboard/admin/ews",
                &session,
                csrf,
                &format!("{base}{extra}"),
            ))
            .await
            .unwrap();
        assert!(
            resp.headers()["location"]
                .to_str()
                .unwrap()
                .contains("ews=override-domains"),
            "{extra}"
        );
    }
    let stored: i64 =
        sqlx::query_scalar("SELECT ews_global_enabled FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 0);

    // Both: saved. Only the users' domain is rewritten.
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!(
                "{base}&ews_impersonation_from_domain=example.com&ews_impersonation_domain=corp.example"
            ),
        ))
        .await
        .unwrap();
    assert!(resp.headers()["location"]
        .to_str()
        .unwrap()
        .contains("ews=saved"));
    let other = insert_role_user(&pool, "jsmith@subsidiary.example", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'S', 'jsmith@subsidiary.example', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&other)
        .execute(&pool)
        .await
        .unwrap();
    trust(&pool, &other).await;
    idp_verify(&pool, &other).await;
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .unwrap();
    crate::ews::global::provision_managed_ews_source_for_all_users(&pool, &cfg)
        .await
        .unwrap();
    assert_eq!(
        managed_source_of(&pool, &other).await.unwrap().1.as_deref(),
        Some("jsmith@subsidiary.example"),
        "an address in another domain keeps its own mailbox"
    );
    assert_eq!(
        managed_source_of(&pool, &admin_id)
            .await
            .unwrap()
            .1
            .as_deref(),
        Some("test@corp.example"),
        "an address in the users' domain gets the mailbox domain"
    );
}

/// Two users whose addresses name one mailbox (here through the override):
/// the second is refused, and the admin card says why.
#[tokio::test]
async fn a_mailbox_already_used_by_another_user_is_refused_and_shown() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, Some(("example.local", "example.com"))).await;
    provision(&pool, &admin_id, "test@example.com").await;
    let alias = insert_role_user(&pool, "test@example.local", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'A', 'test@example.local', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&alias)
        .execute(&pool)
        .await
        .unwrap();
    trust(&pool, &alias).await;
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .unwrap();
    assert_eq!(
        crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &alias)
            .await
            .unwrap(),
        crate::ews::global::Provisioned::Conflict
    );
    assert!(managed_source_of(&pool, &alias).await.is_none());

    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(&admin_id)
    .execute(&pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let app = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("test@example.local"));
    // As rendered: the template escapes the apostrophe.
    let conflict = crate::i18n::translate("en", "admin-ews-row-conflict", None);
    assert!(
        page.contains(&conflict.replace('\'', "&#x27;")),
        "{conflict}"
    );
}

/// The rules are checked again every time a provider is built, not only when
/// the row was provisioned: a suspended or no longer trusted user is skipped
/// (not a failure), and a row that no longer matches its user's address is
/// refused.
#[tokio::test]
async fn the_provider_rechecks_the_user_before_every_use() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    let source = provision(&pool, &admin_id, "test@example.com").await;
    let build = || {
        crate::ews::global::provider_for_source(&pool, &[0u8; 32], &source, EWS_URL, "svc", None)
    };
    assert!(build().await.unwrap().is_some());

    sqlx::query("UPDATE users SET enabled = 0 WHERE id = ?")
        .bind(&admin_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(build().await.unwrap().is_none(), "suspended: skipped");
    sqlx::query("UPDATE users SET enabled = 1, email_trusted = 0 WHERE id = ?")
        .bind(&admin_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(build().await.unwrap().is_none(), "untrusted: skipped");

    sqlx::query("UPDATE users SET email_trusted = 1, email = 'other@example.com' WHERE id = ?")
        .bind(&admin_id)
        .execute(&pool)
        .await
        .unwrap();
    let err = build().await.err().expect("a stale mailbox is refused");
    assert!(err.to_string().contains("out of date"), "{err}");
}

/// The service account is shared and every user can open the sources page.
#[tokio::test]
async fn the_service_account_is_not_shown_to_regular_users() {
    let (app, pool, admin_session, user_id, user_session) = setup_with_global(false, false).await;
    provision(&pool, &user_id, "alice@example.com").await;
    let user_page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/sources", &user_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(user_page.contains("Admin-managed"));
    assert!(!user_page.contains("svc@example.com"));

    let admin_id: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&admin_session)
        .fetch_one(&pool)
        .await
        .unwrap();
    provision(&pool, &admin_id, "test@example.com").await;
    let admin_page = body_string(
        app.oneshot(get_authed("/dashboard/sources", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        admin_page.contains("svc@example.com"),
        "admins still see it"
    );
}

/// A user who never opens the sources page still gets their bookings written
/// to their agenda: the first sync points the managed source at the mailbox's
/// default calendar, found by id since its name follows the mailbox language.
#[tokio::test]
async fn the_first_sync_writes_bookings_to_the_default_calendar() {
    let pool = setup_test_db().await;
    let (host_id, _, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let source_id = provision(&pool, &host_id, "test@example.com").await;
    let write_href = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT write_calendar_href FROM caldav_sources WHERE id = ?",
        )
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(
        write_href().await,
        None,
        "nothing chosen before the first sync"
    );

    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    assert_eq!(write_href().await.as_deref(), Some("CAL-test"));
    assert!(
        ews.calls().iter().any(|c| c.op == "GetFolder"),
        "the default calendar was asked of the server, not guessed by name"
    );

    // And a booking goes there without the user having chosen anything.
    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@example.com",
        Utc::now() + Duration::days(7),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{booking_id}");
    let (details, _) = writeback::writeback_details(&pool, &uid).await.unwrap();
    caldav_push_booking(&pool, &[0u8; 32], &host_id, &uid, &details).await;
    let created = ews.created();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].0, "CAL-test");

    // "Do not write" is a choice: a later sync must not undo it.
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = NULL WHERE id = ?")
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    assert_eq!(
        write_href().await,
        None,
        "an explicit choice survives a sync"
    );

    // A source that starts reading another mailbox gets that mailbox's default.
    provision(&pool, &host_id, "t.user@example.com").await;
    assert_eq!(
        write_href().await,
        None,
        "the old mailbox's calendar is dropped"
    );
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    assert_eq!(write_href().await.as_deref(), Some("CAL-t.user"));
}

/// A source the user added themselves keeps its manual setup: only managed
/// sources are pointed at a default calendar.
#[tokio::test]
async fn a_source_the_user_added_is_not_given_a_write_calendar() {
    let pool = setup_test_db().await;
    let (host_id, account_id, _) = seed_test_data(&pool).await;
    let (endpoint, _ews) = spawn_mock_ews().await;
    let enc = crate::crypto::encrypt_password(&[0u8; 32], "pw").unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type) VALUES (?, ?, 'Mine', ?, 'test@example.com', ?, 'ews')")
        .bind(&id)
        .bind(&account_id)
        .bind(&endpoint)
        .bind(&enc)
        .execute(&pool)
        .await
        .unwrap();
    let _ = host_id;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &id).await;
    let href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(href, None);
}

/// A managed source with nothing to do (global Exchange off, or its user
/// suspended) is neither synced nor written to, and neither counts as a
/// failure: no retry, no alert email, no request to Exchange.
#[tokio::test]
async fn an_idle_managed_source_is_not_synced_or_written_to() {
    let pool = setup_test_db().await;
    let (host_id, _, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let source_id = provision(&pool, &host_id, "test@example.com").await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source_id).await;
    sqlx::query("UPDATE caldav_sources SET write_calendar_href = 'CAL-test', last_synced = NULL WHERE id = ?")
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();

    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@example.com",
        Utc::now() + Duration::days(7),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{booking_id}");
    let (details, _) = writeback::writeback_details(&pool, &uid).await.unwrap();

    for (case, sql) in [
        (
            "global Exchange off",
            "UPDATE auth_config SET ews_global_enabled = 0 WHERE id = 'singleton'",
        ),
        (
            "user suspended",
            "UPDATE auth_config SET ews_global_enabled = 1 WHERE id = 'singleton'; \
             UPDATE users SET enabled = 0 WHERE email = 'test@example.com'",
        ),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        let calls_before = ews.calls().len();

        caldav_push_booking(&pool, &[0u8; 32], &host_id, &uid, &details).await;
        let (error, attempts): (Option<String>, i64) = sqlx::query_as(
            "SELECT writeback_error, writeback_attempts FROM bookings WHERE uid = ?",
        )
        .bind(&uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((error, attempts), (None, 0), "{case}: not a write failure");

        assert_ne!(
            crate::commands::sync::next_background_source(&pool, 0)
                .await
                .as_deref(),
            Some(source_id.as_str()),
            "{case}: not a background candidate"
        );
        crate::commands::sync::sync_if_stale(&pool, &[0u8; 32], &host_id).await;
        assert_eq!(
            ews.calls().len(),
            calls_before,
            "{case}: Exchange is not contacted"
        );
        assert!(ews.created().is_empty(), "{case}: nothing written");
    }
}

/// The admin can take the feature out of service in one step: every managed
/// source goes (calendars and cached busy times with it), the service
/// password is cleared and global Exchange is switched off. Admins only.
#[tokio::test]
async fn removing_managed_sources_clears_them_and_the_service_password() {
    let (app, pool, admin_session, user_id, user_session) = setup_with_global(false, false).await;
    let source_id = provision(&pool, &user_id, "alice@example.com").await;
    let cal_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO calendars (id, source_id, href) VALUES (?, ?, 'CAL-alice')")
        .bind(&cal_id)
        .bind(&source_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events (id, calendar_id, uid, start_at, end_at) VALUES ('ev-a', ?, 'u-a', '20300101T100000', '20300101T110000')")
        .bind(&cal_id)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-ews-remove";

    let refused = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews/remove",
            &user_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(refused.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(managed_source_of(&pool, &user_id).await.is_some());

    let resp = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews/remove",
            &admin_session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    let location = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("ews=removed&n=1"), "{location}");

    let left: (i64, i64, i64, i64, bool) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM caldav_sources WHERE managed = 1), \
                (SELECT COUNT(*) FROM calendars), (SELECT COUNT(*) FROM events), \
                ews_global_enabled, ews_service_password_enc IS NULL \
         FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, (0, 0, 0, 0, true));

    let page = body_string(
        app.oneshot(get_authed(
            "/dashboard/admin?ews=removed&n=1",
            &admin_session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(page.contains("Managed sources removed: 1."));
}

/// Log in an admin for the app built by `setup_test_db` + `seed_test_data`.
async fn admin_app(pool: &SqlitePool, admin_id: &str) -> (Router, String) {
    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, datetime('now', '+1 day'))",
    )
    .bind(&session)
    .bind(admin_id)
    .execute(pool)
    .await
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    (
        create_router(pool.clone(), data_dir, [0u8; 32]).await,
        session,
    )
}

async fn stored_password(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT ews_service_password_enc FROM auth_config WHERE id = 'singleton'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The stored service password belongs to the endpoint and the account it
/// was typed for. Pointing the card at another server or another account
/// without typing it again clears it, so it is never sent to the new host,
/// and the feature idles until it is entered.
#[tokio::test]
async fn a_new_endpoint_or_account_without_a_new_password_clears_it() {
    for (case, url, user) in [
        (
            "endpoint",
            "https%3A%2F%2Fwww.example.com%2FEWS%2FExchange.asmx",
            "svc%40example.com",
        ),
        (
            "account",
            "https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx",
            "other%40example.com",
        ),
    ] {
        let pool = setup_test_db().await;
        let (admin_id, _, _) = seed_test_data(&pool).await;
        store_global_config(&pool, false, false).await;
        let (app, session) = admin_app(&pool, &admin_id).await;
        let csrf = "csrf-ews-move";

        // Same endpoint (written differently) and account: the password stays.
        let resp = app
            .clone()
            .oneshot(post_form(
                "/dashboard/admin/ews",
                &session,
                csrf,
                &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url=HTTPS%3A%2F%2FExample.com%2FEWS%2FExchange.asmx&ews_service_username=svc%40example.com"),
            ))
            .await
            .unwrap();
        let location = resp.headers()["location"].to_str().unwrap().to_string();
        assert!(location.contains("ews=saved"), "{case}: {location}");
        assert!(
            stored_password(&pool).await.is_some(),
            "{case}: unchanged target keeps it"
        );

        let resp = app
            .clone()
            .oneshot(post_form(
                "/dashboard/admin/ews",
                &session,
                csrf,
                &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username={user}"),
            ))
            .await
            .unwrap();
        let location = resp.headers()["location"].to_str().unwrap().to_string();
        assert!(
            location.contains("ews=password-cleared"),
            "{case}: {location}"
        );
        assert_eq!(stored_password(&pool).await, None, "{case}: cleared");
        assert!(
            crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
                .await
                .is_none(),
            "{case}: the feature idles"
        );

        // Typing it again with the new target is accepted.
        let resp = app
            .oneshot(post_form(
                "/dashboard/admin/ews",
                &session,
                csrf,
                &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username={user}&ews_service_password=new-pass"),
            ))
            .await
            .unwrap();
        let location = resp.headers()["location"].to_str().unwrap().to_string();
        assert!(location.contains("ews=saved"), "{case}: {location}");
        assert!(stored_password(&pool).await.is_some());
    }
}

/// The service account's password goes out as Basic auth on every request,
/// so the global endpoint must be https.
#[tokio::test]
async fn the_global_endpoint_must_be_https() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    let (app, session) = admin_app(&pool, &admin_id).await;
    let csrf = "csrf-ews-http";
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url=http%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx&ews_service_username=svc&ews_service_password=pw"),
        ))
        .await
        .unwrap();
    let location = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("ews=https-required"), "{location}");
    let url: Option<String> =
        sqlx::query_scalar("SELECT ews_global_url FROM auth_config WHERE id = 'singleton'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(url, None, "nothing saved");
}

/// Registration stores the address it checked (trimmed), so the duplicate
/// check and the stored address agree; the card shows the mailbox a
/// confirmation would open.
#[tokio::test]
async fn registration_stores_the_trimmed_address_and_the_card_shows_its_mailbox() {
    let (app, pool, admin_session, _, _) = setup_with_global(false, false).await;
    sqlx::query(
        "UPDATE auth_config SET registration_enabled = 1, \
         ews_impersonation_from_domain = 'example.org', ews_impersonation_domain = 'mail.example' \
         WHERE id = 'singleton'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let csrf = "csrf-register-trim";
    let resp = app
        .clone()
        .oneshot(post_form_unauthed(
            "/auth/register",
            csrf,
            &format!(
                "_csrf={csrf}&name=Bob&email=%20%20Bob%40Example.org%20&password=strongpassword123"
            ),
        ))
        .await
        .unwrap();
    assert!(resp.status().is_redirection());
    let stored: Vec<String> =
        sqlx::query_scalar("SELECT email FROM users WHERE lower(email) = 'bob@example.org'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(stored, ["Bob@Example.org"]);
    let accounts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE email = 'Bob@Example.org'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(accounts, 1);

    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("bob@mail.example"),
        "the derived mailbox is shown"
    );
}

/// When an account only an admin vouched for holds a mailbox, the owner the
/// identity provider verified takes it over instead of being refused; the
/// other way round, the verified owner keeps it.
#[tokio::test]
async fn an_idp_verified_owner_takes_over_from_an_admin_confirmed_account() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    // The admin confirmed a local account first.
    let local = provision(&pool, &admin_id, "ceo@example.com").await;
    let owner = insert_role_user(&pool, "CEO@example.com", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'C', 'CEO@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE users SET email_trusted = 1, email_verified_by_idp = 1, auth_provider = 'oidc' WHERE id = ?",
    )
    .bind(&owner)
    .execute(&pool)
    .await
    .unwrap();
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .unwrap();
    assert_eq!(
        crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &owner)
            .await
            .unwrap(),
        crate::ews::global::Provisioned::Created
    );
    assert_eq!(
        managed_source_of(&pool, &owner).await.unwrap().1.as_deref(),
        Some("ceo@example.com")
    );
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
    let gone: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM caldav_sources WHERE id = ?")
        .bind(&local)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(gone, 0);

    // The admin-confirmed account cannot take it back.
    assert_eq!(
        crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &admin_id)
            .await
            .unwrap(),
        crate::ews::global::Provisioned::Conflict
    );
    assert!(managed_source_of(&pool, &owner).await.is_some());
}

/// A provisioning that read the config before the admin switched the
/// feature off writes nothing: the feature is checked in the same statement.
#[tokio::test]
async fn provisioning_rechecks_the_feature_when_it_writes() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    trust(&pool, &admin_id).await;
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .unwrap();
    sqlx::query(
        "UPDATE auth_config SET ews_global_enabled = 0, ews_service_password_enc = NULL \
         WHERE id = 'singleton'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &admin_id)
            .await
            .unwrap(),
        crate::ews::global::Provisioned::Inactive
    );
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
}

/// A sync whose client still impersonates the mailbox the source read when
/// it started stores nothing once the source points elsewhere; a calendar
/// the mailbox no longer lists is dropped; folder names are not stored.
#[tokio::test]
async fn a_sync_of_a_mailbox_the_source_left_stores_nothing() {
    let pool = setup_test_db().await;
    let (host_id, _, _) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let day = (Utc::now().naive_utc().date() + Duration::days(2))
        .format("%Y-%m-%dT08:00:00Z")
        .to_string();
    ews.set_busy(&day, &day.replace("T08:", "T09:"));
    let source = provision(&pool, &host_id, "test@example.com").await;
    let old_client =
        crate::ews::global::provider_for_source(&pool, &[0u8; 32], &source, &endpoint, "svc", None)
            .await
            .unwrap()
            .unwrap();

    // Re-pointed (an address change) while the old client is still in use.
    provision(&pool, &host_id, "t.user@example.com").await;
    crate::commands::sync::sync_ews_source(&pool, &[0u8; 32], old_client.as_ref(), &source)
        .await
        .unwrap();
    let calendars: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
        .bind(&source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calendars, 0, "the old mailbox's calendars do not come back");

    // A folder the new mailbox does not list goes (after two listings in a
    // row); the listed one gets a neutral name, not its folder name.
    sqlx::query("INSERT INTO calendars (id, source_id, href, display_name) VALUES ('gone', ?, 'CAL-gone', 'Old')")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source).await;
    sqlx::query("UPDATE caldav_sources SET last_synced = NULL WHERE id = ?")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source).await;
    let names: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT href, display_name FROM calendars WHERE source_id = ?")
            .bind(&source)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        names,
        [(
            "CAL-t.user".to_string(),
            Some("Exchange calendar 1".to_string())
        )]
    );
}

/// An admin can remove one user's managed source from the status table;
/// regular users cannot.
#[tokio::test]
async fn an_admin_can_remove_one_users_managed_source() {
    let (app, pool, admin_session, user_id, user_session) = setup_with_global(false, false).await;
    provision(&pool, &user_id, "alice@example.com").await;
    let csrf = "csrf-ews-remove-user";
    let page = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/admin", &admin_session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("/dashboard/admin/ews/remove-user"));
    let refused = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/ews/remove-user",
            &user_session,
            csrf,
            &format!("_csrf={csrf}&user_id={user_id}"),
        ))
        .await
        .unwrap();
    assert_eq!(refused.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(managed_source_of(&pool, &user_id).await.is_some());
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews/remove-user",
            &admin_session,
            csrf,
            &format!("_csrf={csrf}&user_id={user_id}"),
        ))
        .await
        .unwrap();
    assert!(resp.headers()["location"]
        .to_str()
        .unwrap()
        .contains("ews=source-removed"));
    assert!(managed_source_of(&pool, &user_id).await.is_none());
}

/// Testing an idle managed source says why it is idle.
#[tokio::test]
async fn an_idle_managed_source_says_why() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    let source = provision(&pool, &admin_id, "test@example.com").await;
    let reason = || crate::ews::global::idle_reason(&pool, &[0u8; 32], &source);
    assert_eq!(reason().await, None);
    sqlx::query("UPDATE users SET email_trusted = 0 WHERE id = ?")
        .bind(&admin_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(reason().await, Some("sources-managed-user-untrusted"));
    sqlx::query("UPDATE users SET email_trusted = 1, enabled = 0 WHERE id = ?")
        .bind(&admin_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(reason().await, Some("sources-managed-user-suspended"));
    sqlx::query("UPDATE auth_config SET ews_global_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(reason().await, Some("sources-managed-global-disabled"));
}

/// Seed a cached calendar on `source`, selected by event type `et_id`.
async fn seed_selected_calendar(pool: &SqlitePool, source: &str, et_id: &str) {
    let cal = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO calendars (id, source_id, href, display_name) VALUES (?, ?, 'CAL-x', 'Exchange calendar 1')")
        .bind(&cal)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)")
        .bind(et_id)
        .bind(&cal)
        .execute(pool)
        .await
        .unwrap();
}

async fn calendar_count(pool: &SqlitePool, source: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM calendars WHERE source_id = ?")
        .bind(source)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Writing the endpoint differently (path case) on the same server keeps
/// what the managed sources cached; another server resets them, and the
/// notice counts the event types that lost a calendar selection.
#[tokio::test]
async fn only_a_new_server_resets_the_managed_sources() {
    let pool = setup_test_db().await;
    let (admin_id, _, et_id) = seed_test_data(&pool).await;
    store_global_config(&pool, false, false).await;
    let source = provision(&pool, &admin_id, "test@example.com").await;
    seed_selected_calendar(&pool, &source, &et_id).await;
    let (app, session) = admin_app(&pool, &admin_id).await;
    let csrf = "csrf-ews-server";
    let save = |url: &'static str| {
        let app = app.clone();
        let session = session.clone();
        async move {
            let resp = app
                .oneshot(post_form(
                    "/dashboard/admin/ews",
                    &session,
                    csrf,
                    &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url={url}&ews_service_username=svc%40example.com&ews_service_password=pw"),
                ))
                .await
                .unwrap();
            resp.headers()["location"].to_str().unwrap().to_string()
        }
    };
    let location = save("https%3A%2F%2Fexample.com%2Fews%2Fexchange.asmx").await;
    assert!(!location.contains("lost="), "{location}");
    assert_eq!(calendar_count(&pool, &source).await, 1, "same server: kept");

    let location = save("https%3A%2F%2Fwww.example.com%2FEWS%2FExchange.asmx").await;
    assert!(location.contains("lost=1"), "{location}");
    assert_eq!(
        calendar_count(&pool, &source).await,
        0,
        "another server: reset"
    );
}

/// The eviction of an admin-confirmed holder and the IdP-verified owner's
/// insert are one transaction: when the insert writes nothing (the feature
/// switched off meanwhile), the holder keeps the mailbox. When it goes
/// through, the status table says from whom it was taken.
#[tokio::test]
async fn a_takeover_is_all_or_nothing_and_shown() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    provision(&pool, &admin_id, "ceo@example.com").await;
    let owner = insert_role_user(&pool, "CEO@example.com", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'C', 'CEO@example.com', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET email_trusted = 1, email_verified_by_idp = 1 WHERE id = ?")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    let cfg = crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
        .await
        .unwrap();

    sqlx::query("UPDATE auth_config SET ews_global_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &owner)
            .await
            .unwrap(),
        crate::ews::global::Provisioned::Inactive
    );
    assert!(
        managed_source_of(&pool, &admin_id).await.is_some(),
        "the holder keeps it"
    );

    sqlx::query("UPDATE auth_config SET ews_global_enabled = 1 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    crate::ews::global::provision_managed_ews_source_for_user(&pool, &cfg, &owner)
        .await
        .unwrap();
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
    let (app, session) = admin_app(&pool, &admin_id).await;
    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("Taken over from ceo@example.com on"),
        "status shows it"
    );
}

/// A new domain override that maps two sources onto one mailbox gives it to
/// the owner the identity provider verified, even when the other account is
/// older; and an address trusted on an admin's word only must be confirmed
/// again when the override changes the mailbox it opens.
#[tokio::test]
async fn a_new_override_prefers_the_verified_owner_and_asks_for_reconfirmation() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    // Older, admin-confirmed: jsmith@example.com.
    provision(&pool, &admin_id, "jsmith@example.com").await;
    // Newer, IdP-verified: jsmith@example.local, which the new override maps
    // onto jsmith@example.com.
    let owner = insert_role_user(&pool, "jsmith@example.local", "user").await;
    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'J', 'jsmith@example.local', 'UTC', ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET email_trusted = 1, email_verified_by_idp = 1 WHERE id = ?")
        .bind(&owner)
        .execute(&pool)
        .await
        .unwrap();
    // A third, admin-confirmed address the override re-maps.
    let local = insert_role_user(&pool, "ann@example.local", "user").await;
    trust(&pool, &local).await;
    crate::ews::global::provision_managed_ews_source_for_user(
        &pool,
        &crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
            .await
            .unwrap(),
        &owner,
    )
    .await
    .unwrap();

    let new = crate::ews::global::DomainOverride::new(Some("example.local"), Some("example.com"));
    assert_eq!(
        crate::ews::global::untrust_remapped(
            &mut pool.acquire().await.unwrap(),
            None,
            new.as_ref()
        )
        .await
        .unwrap(),
        1,
        "only the admin-confirmed address the override re-maps"
    );
    let ann_trusted: bool = sqlx::query_scalar("SELECT email_trusted FROM users WHERE id = ?")
        .bind(&local)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!ann_trusted);
    crate::ews::global::retarget_managed_sources(&pool, new.as_ref())
        .await
        .unwrap();
    assert_eq!(
        managed_source_of(&pool, &owner).await.unwrap().1.as_deref(),
        Some("jsmith@example.com"),
        "the verified owner gets the mailbox"
    );
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
}

/// Removing a managed source waits for its mailbox lock, so it never runs
/// in the middle of a sync storing what it fetched.
#[tokio::test]
async fn removing_a_managed_source_waits_for_its_lock() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_config_for(&pool, EWS_URL, None).await;
    let source = provision(&pool, &admin_id, "test@example.com").await;
    let lock = crate::ews::global::mailbox_lock(&source);
    let guard = lock.lock().await;
    let task = {
        let pool = pool.clone();
        let admin_id = admin_id.clone();
        tokio::spawn(async move {
            crate::ews::global::remove_managed_source(&pool, &admin_id)
                .await
                .unwrap()
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        managed_source_of(&pool, &admin_id).await.is_some(),
        "still there while the lock is held"
    );
    drop(guard);
    assert_eq!(task.await.unwrap(), 1);
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
}

/// A database written before the https rule cannot send the service
/// password in clear: only loopback http (a local test server) is accepted.
#[tokio::test]
async fn a_stored_http_endpoint_leaves_the_feature_off() {
    for (url, ok) in [
        ("https://mail.example.com/EWS/Exchange.asmx", true),
        ("http://mail.example.com/EWS/Exchange.asmx", false),
        ("http://127.0.0.1:8080/EWS/Exchange.asmx", true),
        ("http://[::1]/EWS/Exchange.asmx", true),
        ("ftp://mail.example.com/", false),
    ] {
        assert_eq!(crate::ews::global::endpoint_is_acceptable(url), ok, "{url}");
    }
    let pool = setup_test_db().await;
    store_config_for(&pool, "http://mail.example.com/EWS/Exchange.asmx", None).await;
    assert!(
        crate::ews::global::load_ews_global_config(&pool, &[0u8; 32])
            .await
            .is_none()
    );
}

/// Per-event-type write destinations (#47) treat a managed Exchange
/// calendar like the user's own only while its source is active: an owner
/// whose address is not trusted cannot pick it, and while the feature is
/// off a booking whose destination is that calendar is neither written nor
/// recorded as a failure.
#[tokio::test]
async fn a_managed_calendar_is_a_destination_only_while_active() {
    let pool = setup_test_db().await;
    let (host_id, _, et_id) = seed_test_data(&pool).await;
    let (endpoint, ews) = spawn_mock_ews().await;
    store_config_for(&pool, &endpoint, None).await;
    let source = provision(&pool, &host_id, "test@example.com").await;
    crate::commands::sync::sync_source_by_id(&pool, &[0u8; 32], &source).await;
    let cal_id: String = sqlx::query_scalar("SELECT id FROM calendars WHERE source_id = ?")
        .bind(&source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(is_destination_choice(&pool, &host_id, &cal_id).await);

    sqlx::query("UPDATE users SET email_trusted = 0 WHERE id = ?")
        .bind(&host_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!is_destination_choice(&pool, &host_id, &cal_id).await);
    assert!(destination_choices(&pool, &host_id)
        .await
        .iter()
        .all(|(id, _)| *id != cal_id));
    trust(&pool, &host_id).await;
    assert!(set_destination(&pool, &host_id, &et_id, &cal_id)
        .await
        .is_ok());

    // Feature off: the booking whose destination is that calendar is not
    // written there, and nothing is recorded as a failure or a copy.
    sqlx::query("UPDATE auth_config SET ews_global_enabled = 0 WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();
    let booking_id = insert_v1_booking(
        &pool,
        &et_id,
        "guest@example.com",
        Utc::now() + Duration::days(7),
        30,
        "confirmed",
        &[],
    )
    .await;
    let uid = format!("uid-{booking_id}");
    let (details, _) = writeback::writeback_details(&pool, &uid).await.unwrap();
    caldav_push_booking(&pool, &[0u8; 32], &host_id, &uid, &details).await;
    let (error, attempts, copies): (Option<String>, i64, i64) = sqlx::query_as(
        "SELECT writeback_error, writeback_attempts, \
                (SELECT COUNT(*) FROM booking_write_copies WHERE booking_uid = ?) \
         FROM bookings WHERE uid = ?",
    )
    .bind(&uid)
    .bind(&uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((error, attempts, copies), (None, 0, 0));
    assert!(ews.created().is_empty());
}

/// Saving a domain override that re-maps an admin-confirmed address writes
/// the override and withdraws that trust together: the redirect counts it,
/// the address is no longer trusted and its managed source is gone.
#[tokio::test]
async fn saving_an_override_asks_to_reconfirm_remapped_addresses() {
    let pool = setup_test_db().await;
    let (admin_id, _, _) = seed_test_data(&pool).await;
    store_global_config(&pool, false, false).await;
    provision(&pool, &admin_id, "test@example.com").await;
    let (app, session) = admin_app(&pool, &admin_id).await;
    let csrf = "csrf-ews-reconfirm";
    let resp = app
        .oneshot(post_form(
            "/dashboard/admin/ews",
            &session,
            csrf,
            &format!("_csrf={csrf}&ews_global_enabled=on&ews_global_url=https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx&ews_service_username=svc%40example.com&ews_impersonation_from_domain=example.com&ews_impersonation_domain=corp.example"),
        ))
        .await
        .unwrap();
    let location = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("reconfirm=1"), "{location}");
    let (trusted, from): (bool, Option<String>) = sqlx::query_as(
        "SELECT (SELECT email_trusted FROM users WHERE id = ?), \
                (SELECT ews_impersonation_from_domain FROM auth_config WHERE id = 'singleton')",
    )
    .bind(&admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((trusted, from.as_deref()), (false, Some("example.com")));
    assert!(managed_source_of(&pool, &admin_id).await.is_none());
}

/// A user's own EWS source pointed at another account reads another
/// mailbox: what it cached goes at once, not one sync later.
#[tokio::test]
async fn editing_an_own_ews_source_to_another_account_drops_its_calendars() {
    let pool = setup_test_db().await;
    let (host_id, account_id, et_id) = seed_test_data(&pool).await;
    let enc = crate::crypto::encrypt_password(&[0u8; 32], "pw").unwrap();
    let source = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, provider_type, write_calendar_href) \
         VALUES (?, ?, 'Mine', 'https://example.com/EWS/Exchange.asmx', 'me@example.com', ?, 'ews', 'CAL-x')",
    )
    .bind(&source)
    .bind(&account_id)
    .bind(&enc)
    .execute(&pool)
    .await
    .unwrap();
    seed_selected_calendar(&pool, &source, &et_id).await;
    let (app, session) = admin_app(&pool, &host_id).await;
    let csrf = "csrf-ews-own";
    let edit = |user: &'static str| {
        let app = app.clone();
        let session = session.clone();
        let source = source.clone();
        async move {
            app.oneshot(post_form(
                &format!("/dashboard/sources/{source}/edit"),
                &session,
                csrf,
                &format!("_csrf={csrf}&name=Mine&url=https%3A%2F%2Fexample.com%2FEWS%2FExchange.asmx&username={user}&password=&no_test=on"),
            ))
            .await
            .unwrap()
        }
    };
    edit("Me%40example.com").await;
    assert_eq!(
        calendar_count(&pool, &source).await,
        1,
        "same account: kept"
    );
    edit("boss%40example.com").await;
    assert_eq!(
        calendar_count(&pool, &source).await,
        0,
        "another account: dropped"
    );
    let write: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(write, None);
}
